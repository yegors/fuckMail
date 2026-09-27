// MailBackend's action toast and Undo: holding a Send for its Undo interval
// before dispatch, and the Undo offered after mail actions: archive or delete,
// labels, Done, Delay and read state.
#include "MailBackend.h"
#include <QJsonArray>
#include <QUuid>
#include <algorithm>

// One 50 ms timer drives toast expiry, held sends and calendar holds. Deadlines
// use m_actionClock, a monotonic clock, so wall-clock changes cannot fire them.
void MailBackend::initializeSendUndo() {
    m_actionClock.start();
    m_actionTimer.setInterval(50);
    m_actionTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_actionTimer, &QTimer::timeout, this, &MailBackend::advanceSendUndo);
}
bool MailBackend::sendQueued() const { return m_queuedSends.contains(googleSendKey()); }
QVariantMap MailBackend::actionToast() const {
    auto toast = m_actionToast;
    const auto key = toast.value("key").toString();
    const bool progress = toast.value("progress").toBool() && (m_queuedSends.contains(key) || m_sendNoticeRecipients.contains(key));
    toast["progress"] = progress;
    toast["visible"] = !m_actionToast.isEmpty() && (progress || m_actionClock.elapsed() < m_actionToastDeadline);
    toast["action_label"] = toast.value("action").toString().startsWith("undo_") ? "Undo" : toast.value("action") == "open_draft" ? "Open draft" : toast.value("action") == "calendar_activity" ? "View activity" : "";
    return toast;
}
void MailBackend::showActionToast(const QString &text, const QString &action, const QVariantMap &context, int duration, bool preserveUndo) {
    auto toast = context; toast["text"] = text; toast["action"] = action; toast["duration"] = duration;
    if (!toast.contains("issued_at")) toast["issued_at"] = m_actionClock.elapsed();
    // With `preserveUndo`, a live Undo toast is not replaced by a plain notice
    // or an older Undo; those wait in a short queue (at most 8) instead.
    if (preserveUndo && m_actionToast.value("action").toString().startsWith("undo_") && m_actionClock.elapsed() < m_actionToastDeadline
        && (!action.startsWith("undo_") || toast.value("issued_at").toLongLong() <= m_actionToast.value("issued_at").toLongLong())) {
        if (m_deferredActionToasts.size() >= 8) m_deferredActionToasts.removeFirst();
        m_deferredActionToasts.append(toast); return;
    }
    m_actionToast = toast; m_actionToastDeadline = m_actionClock.elapsed() + duration;
    m_actionTimer.start(); emit actionToastChanged();
}
void MailBackend::activateActionToast() {
    if (m_actionClock.elapsed() >= m_actionToastDeadline) return;
    if (m_actionToast.value("action") == "undo_discard" && m_undoingDiscards) return;
    if (m_actionToast.value("action") == "undo_mail" && (m_mutatingConversation || !m_connected || m_shutdown)) return;
    const auto toast = m_actionToast;
    m_actionToast = {}; emit actionToastChanged();
    if (toast.value("action") == "undo_mail") {
        undoMailActions(toast);
    } else if (toast.value("action") == "undo_send") {
        const auto key = toast.value("key").toString();
        if (m_queuedSends.contains(key) && m_actionClock.elapsed() < m_queuedSends[key].deadline)
            cancelQueuedSend(key, true);
    } else if (toast.value("action") == "undo_discard") {
        undoDraftDiscards(toast.value("drafts").toList());
    } else if (toast.value("action") == "undo_calendar_delete") {
        undoCalendarDeletion(toast.value("key").toString());
    } else if (toast.value("action") == "calendar_activity") {
        emit calendarActivityRequested();
    } else if (toast.value("action") == "open_draft") {
        openDraftInAccount(toast.value("account").toString(), toast.value("draft_id").toString());
    }
}
// A negative Undo interval sends at once. Zero still goes through the queue, so
// the send gets the same pre-dispatch check, but without an Undo toast.
void MailBackend::queueGoogleSend(const QJsonObject &params) {
    if (m_sendUndoMs < 0) { startGoogleSendOperation("google_send", params); return; }
    const auto key = googleSendKey();
    if (m_queuedSends.contains(key)) return;
    if (m_queuedSends.size() >= 8) { fail("Wait for a pending send before sending another message."); return; }
    QStringList names;
    for (const auto &value : m_draft.value("to_recipients").toList()) {
        const auto recipient = value.toMap();
        names.append(recipient.value("name").toString().isEmpty() ? recipient.value("email").toString() : recipient.value("name").toString());
    }
    QueuedSend send;
    send.preview = queuedOutgoing(params);
    send.params = params; send.account = m_account; send.from = m_draft.value("from").toString();
    send.recipientNames = names.isEmpty() ? QStringLiteral("your recipients") : names.join(", ");
    send.lifecycle = lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong();
    send.deadline = m_actionClock.elapsed() + m_sendUndoMs;
    m_queuedSends.insert(key, send);
    refreshOutgoingPresentation();
    remember("replyOpen", false); emit presentationChanged(); emit draftChanged();
    if (m_sendUndoMs > 0) showActionToast("Sending to " + send.recipientNames, "undo_send", {{"key", key}}, m_sendUndoMs);
    else {
        const bool attachments = !m_draft.value("attachment_ids").toList().isEmpty();
        showActionToast(attachments ? "Sending with attachments…" : "Sending to " + send.recipientNames,
            {}, {{"key", key}, {"progress", true}}, 2800);
        dispatchQueuedSend(key);
    }
}
void MailBackend::cancelQueuedSend(const QString &key, bool restore) {
    if (!m_queuedSends.contains(key)) return;
    const auto send = m_queuedSends.take(key);
    refreshOutgoingPresentation();
    m_googleSendUi[key].feedback = restore ? "Send undone. Your saved draft is kept." : "Send cancelled. Your saved draft is kept.";
    if (m_actionToast.value("key") == key) { m_actionToast = {}; emit actionToastChanged(); }
    emit draftChanged();
    if (restore && !accountRemoved(send.account)
        && lifecycleAccount(send.account).value("lifecycle").toMap().value("revision").toLongLong() == send.lifecycle) {
        openDraftInAccount(send.account, send.params["draft_id"].toString());
    }
}
void MailBackend::cancelQueuedSends(const QString &reason) {
    if (m_queuedSends.isEmpty()) return;
    for (auto it = m_queuedSends.cbegin(); it != m_queuedSends.cend(); ++it) m_googleSendUi[it.key()].feedback = reason;
    m_queuedSends.clear(); m_deferredActionToasts.clear();
    refreshOutgoingPresentation();
    m_actionToast = {}; m_actionTimer.stop(); emit actionToastChanged(); emit draftChanged();
}
void MailBackend::advanceSendUndo() {
    advanceCalendarDeletions();
    const auto now = m_actionClock.elapsed();
    if ((!m_actionToast.isEmpty() && now >= m_actionToastDeadline && !actionToast().value("progress").toBool()) || (m_actionToast.isEmpty() && !m_deferredActionToasts.isEmpty())) {
        m_actionToast = {};
        if (!m_deferredActionToasts.isEmpty()) {
            m_actionToast = m_deferredActionToasts.takeFirst();
            m_actionToastDeadline = now + m_actionToast.value("duration", 2800).toInt();
        }
        emit actionToastChanged();
    }
    const auto keys = m_queuedSends.keys();
    for (const auto &key : keys) {
        if (!m_queuedSends.contains(key)) continue;
        const auto send = m_queuedSends.value(key);
        if (!m_connected || m_shutdown || accountRemoved(send.account)
            || send.lifecycle != lifecycleAccount(send.account).value("lifecycle").toMap().value("revision").toLongLong()) {
            cancelQueuedSend(key, false);
            showActionToast("Pending send cancelled because its account changed.");
        } else if (now >= send.deadline && now >= send.nextCheck && !send.checking) dispatchQueuedSend(key);
    }
    if (m_queuedSends.isEmpty() && m_calendarDeletions.isEmpty() && m_actionToast.isEmpty() && m_deferredActionToasts.isEmpty()) m_actionTimer.stop();
}
// Re-reads the draft when the Undo window ends and sends only if it is still
// the exact saved version Send was pressed on, with no conflicting Google work.
// While its Gmail copy is uploading, it re-checks every 500 ms for up to 30 s.
void MailBackend::dispatchQueuedSend(const QString &key) {
    if (!m_queuedSends.contains(key)) return;
    const auto send = m_queuedSends.value(key);
    m_queuedSends[key].checking = true;
    refreshOutgoingPresentation();
    const auto draftId = send.params["draft_id"].toString();
    request("draft_get", send.account, {{"draft_id", draftId}}, [this, key, send, draftId](auto result, bool ok) {
        // Undo/close may run while this read is pending. Its reply cannot arm a
        // removed action or use whichever mailbox is now selected.
        if (!m_queuedSends.contains(key) || m_queuedSends[key].params != send.params) return;
        m_queuedSends[key].checking = false;
        auto stop = [this, key, send, draftId](const QString &text) {
            m_queuedSends.remove(key); refreshOutgoingPresentation(); m_googleSendUi[key].feedback = text; emit draftChanged();
            showActionToast(text, "open_draft", {{"account", send.account}, {"draft_id", draftId}}, 6000, true);
        };
        if (!m_connected || m_shutdown || accountRemoved(send.account)
            || lifecycleAccount(send.account).value("lifecycle").toMap().value("revision").toLongLong() != send.lifecycle) {
            stop("Pending send cancelled because its account changed."); return;
        }
        if (!ok || result["account"] != send.account || result["draft_id"] != draftId
            || result["revision"] != send.params["expected_revision"] || result["from"] != send.from
            || result["discarded"].toBool() || result["read_only"].toBool()) {
            stop("The draft changed or could not be checked. Review it before sending."); return;
        }
        const auto google = result["google"].toObject(), submission = result["google_send"].toObject();
        const auto state = google["state"].toString(), sendState = submission["state"].toString();
        if (state == "prepared" || state == "in_flight" || state == "acknowledged") {
            if (m_actionClock.elapsed() - send.deadline >= 30000) { stop("Draft sync is still busy. Your draft is kept; try Send when it finishes."); return; }
            m_queuedSends[key].nextCheck = m_actionClock.elapsed() + 500; return;
        }
        if (state == "conflict" || state == "acceptance_unknown" || sendState == "prepared"
            || sendState == "in_flight" || sendState == "acceptance_unknown"
            || submission["last_accepted_draft_revision"].toInteger() >= send.params["expected_revision"].toInteger()) {
            stop("Check this draft's existing Google activity before sending."); return;
        }
        auto params = send.params;
        // This request has never been invoked. Background draft synchronization
        // may advance its Google revision without changing the authored version.
        params["expected_google_revision"] = std::max(google["revision"].toInteger(), submission["current_google_revision"].toInteger());
        m_queuedSends.remove(key); m_sendNoticeRecipients[key] = send.recipientNames;
        auto preview = send.preview; preview["state"] = "in_flight";
        m_transientOutgoing[send.account + '\n' + preview.value("request_id").toString()] = preview;
        refreshOutgoingPresentation();
        startGoogleSendForDraft("google_send", params, send.account, draftId);
        m_sendProgressTimer.start();
        emit draftChanged();
    }, true);
}
void MailBackend::finishQueuedSendNotice(const QString &key, const QJsonObject &result, bool valid) {
    if (!m_sendNoticeRecipients.contains(key)) return;
    const auto names = m_sendNoticeRecipients.take(key);
    const auto account = key.section('\n', 0, 0), draft = key.section('\n', 1);
    if (valid && result["state"] == "accepted") showActionToast("Sent to " + names, {}, {{"success",true}}, 2800, true);
    else showActionToast("Send needs attention. Your draft is kept.", "open_draft", {{"account", account}, {"draft_id", draft}}, 6000, true);
}

QString MailBackend::mailActionLabel(const QString &method, const QJsonObject &params, int count) const {
    QString label;
    if (method == "mail_filing") label=params["action"]=="trash" ? "Deleted" : params["action"]=="restore" ? "Restored to Inbox" : "Archived";
    else if (method == "set_category") label=params["present"].toBool() ? "Label added" : "Label removed";
    else if (params.contains("read") && params.contains("starred")) label="Marked done";
    else if (params.contains("starred")) label=params["starred"].toBool() ? "Delayed" : "Back in To review";
    else label=params["read"].toBool() ? "Marked read" : "Marked unread";
    return label+(count>1 ? " · "+QString::number(count)+" conversations" : "");
}
QVariantMap MailBackend::mailUndoToken(const QString &account, const QJsonObject &result) const {
    const auto undo=result["mail_undo"].toObject();
    if (undo["version"] != 1 || undo["request_id"].toString().isEmpty() || result["id"].toString().isEmpty() || undo["revision"]!=result["revision"]) return {};
    return {{"account",account},{"id",result["id"].toString()},{"undo_request_id",undo["request_id"].toString()},{"expected_revision",result["revision"].toVariant()},
        {"lifecycle",lifecycleAccount(account).value("lifecycle").toMap().value("revision")}};
}
void MailBackend::offerMailUndo(const QString &label, const QVariantList &tokens, const QVariantList &selection) {
    if (tokens.isEmpty()) return;
    showActionToast(label,"undo_mail",{{"tokens",tokens},{"selection",selection}},6000);
}
void MailBackend::dismissMailUndo() {
    ++m_mailUndoEpoch;
    if (m_actionToast.value("action")=="undo_mail") {m_actionToast={};emit actionToastChanged();}
    m_deferredActionToasts.removeIf([](const auto &v){return v.value("action")=="undo_mail";});
}
void MailBackend::undoMailActions(const QVariantMap &context) {
    if (m_mutatingConversation || !m_connected || m_shutdown) return;
    m_pendingMailUndo=context.value("tokens").toList();
    m_mailUndoSelection=context.value("selection").toList();m_restoredMailSelection.clear();
    if (m_pendingMailUndo.isEmpty() || m_pendingMailUndo.size()>200) {m_pendingMailUndo.clear();return;}
    m_mutatingConversation=true;emit mutationChanged();runMailUndo();
}
// Undoes one conversation per request, in order, and stops at the first
// failure; each token is bound to the revision its action produced.
void MailBackend::runMailUndo() {
    if (m_pendingMailUndo.isEmpty()) {
        m_mutatingConversation=false;emit mutationChanged();loadAccounts();loadCategories();loadConversations(false,true);
        if (!m_mailUndoSelection.isEmpty()) emit mailSelectionRestored(m_restoredMailSelection);
        m_mailUndoSelection.clear();m_restoredMailSelection.clear();showActionToast("Undone",{},{},2800);return;
    }
    const auto token=m_pendingMailUndo.takeFirst().toMap();
    const auto account=token.value("account").toString(), id=token.value("id").toString();
    if (!m_connected || m_shutdown || accountRemoved(account) || !accountCapability(account,"mail_write") || token.value("lifecycle").toLongLong()!=lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) {
        m_pendingMailUndo.clear();m_mutatingConversation=false;emit mutationChanged();showActionToast("Undo stopped because account access changed",{},{},6000);return;
    }
    QJsonObject params{{"id",id},{"undo_request_id",token.value("undo_request_id").toString()},{"expected_revision",token.value("expected_revision").toLongLong()},{"request_id",QUuid::createUuid().toString(QUuid::WithoutBraces)}};
    request("mail_undo",account,params,[this,account,id,token](auto result,bool ok) {
        if (!ok || result["account"]!=account || result["id"]!=id || result["undone_request_id"]!=token.value("undo_request_id").toString()) {m_mailUndoSelection.clear();m_restoredMailSelection.clear();m_pendingMailUndo.clear();m_mutatingConversation=false;emit mutationChanged();showActionToast("Could not undo: "+result["message"].toString(),{},{},6000);loadConversations(false,true);return;}
        auto row=retainReaderPresentation(result.toVariantMap());
        cacheConversation(account,id,row,token.value("lifecycle").toLongLong());
        row["selection_account"]=account;row["selection_lifecycle"]=token.value("lifecycle");
        const auto messages=row.value("messages").toList();
        row["unread"]=std::any_of(messages.cbegin(),messages.cend(),[](const auto &v){return v.toMap().value("unread").toBool();});
        row["starred"]=std::any_of(messages.cbegin(),messages.cend(),[](const auto &v){return v.toMap().value("starred").toBool();});
        m_restoredMailSelection.append(row);
        if (account==m_account && id==m_selected) {m_conversation=row;m_pinnedRow.clear();m_pinnedIndex=-1;emit conversationChanged();}
        if (result["delivery"].isObject()) {m_mailDelivery[qMakePair(account,id)]=result["delivery"].toObject().toVariantMap();emit mailDeliveryChanged();}
        runMailUndo();
    });
}
