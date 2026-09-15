#include "MailBackend.h"
#include <QJsonArray>

bool MailBackend::draftDiscardPending() const { return m_draftDiscardUi.contains(googleSendKey()) || m_bulkDiscardTargets.contains(googleSendKey()); }
bool MailBackend::draftDiscardActive() const { return m_draftDiscardUi.value(googleSendKey()).active; }
QVariantMap MailBackend::draftDiscard() const {
    const auto ui = m_draftDiscardUi.value(googleSendKey());
    const bool ready = !ui.params.isEmpty() && m_connected && !m_shutdown && !accountArchived() && !ui.active
        && lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong() == ui.lifecycle;
    return {{"visible", !ui.params.isEmpty()}, {"status", ui.active ? "Saving the draft action…" : ui.feedback},
        {"can_check", ready && ui.method == "draft_discard"}, {"can_retry", ready && (ui.notFound || ui.method == "draft_undo_discard")},
        {"retry_label", ui.method == "draft_undo_discard" ? "Retry Undo" : "Retry discard"}};
}
QJsonObject MailBackend::discardEditorBuffer() const {
    QJsonObject buffer;
    for (const auto field : {"body", "body_html", "signature_text", "subject", "to_recipients", "cc_recipients", "bcc_recipients", "attachment_ids"})
        buffer[field] = QJsonValue::fromVariant(m_draft.value(field));
    buffer["recipient_inputs"] = QJsonObject::fromVariantMap(recipientInputs());
    return buffer;
}
QVariantMap MailBackend::draftDiscardContext() const {
    return {{"account", m_account}, {"draft_id", m_draft.value("draft_id")},
        {"lifecycle", lifecycleAccount(m_account).value("lifecycle").toMap().value("revision")},
        {"authored", googleSendAuthorSnapshot().toVariantMap()}};
}
QVariantMap MailBackend::discardUndoToken(const QString &account, const QJsonObject &result, bool reopen) const {
    const auto undo = result["discard_undo"].toObject();
    if (undo["version"] != 1 || undo["request_id"].toString().isEmpty() || undo["draft_id"] != result["draft_id"] || undo["revision"] != result["revision"])
        return {};
    return {{"account", account}, {"draft_id", undo["draft_id"].toVariant()}, {"discard_request_id", undo["request_id"].toVariant()},
        {"revision", undo["revision"].toVariant()}, {"lifecycle", undo["lifecycle_revision"].toVariant()}, {"reopen", reopen}};
}
void MailBackend::discardDraft(const QVariantMap &context) {
    if (!context.isEmpty() && context != draftDiscardContext()) { fail("The draft changed. Review it before discarding."); return; }
    if (!m_connected || m_shutdown || draftLocked() || m_draft.isEmpty() || draftBusy() || attachmentImportPending()) return;
    cancelPendingGoogleSend("Send cancelled before discarding the draft.");
    if (m_draft.value("revision").toLongLong() == 0 && m_draftDirty) {
        const auto account = m_account, selected = draftContext();
        m_afterDraftSave = [this, account, selected] { if (account == m_account && selected == draftContext()) discardDraft(); };
        saveDraft(); return;
    }
    m_draftTimer.stop();
    auto params = draftSelector(); params["request_id"] = mutation()["request_id"]; params["expected_revision"] = m_draft.value("revision").toLongLong();
    if (m_draft.value("revision").toLongLong() > 0) params["buffer"] = discardEditorBuffer();
    const auto account = m_account; const auto issued = m_actionClock.elapsed();
    startDraftDiscardRequest("draft_discard", account, params, issued, true, [this, account, issued](auto result, bool ok) {
        if (ok) {
            const auto token = discardUndoToken(account, result, true);
            if (token.isEmpty()) showActionToast("Draft discarded"); else offerDiscardUndo({token}, issued);
            loadAccounts(); if (m_folder == "drafts") loadDraftRows(false);
            if (m_afterDraftSave) saveDraft();
        }
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::startDraftDiscardRequest(const QString &method, const QString &account, const QJsonObject &params, qint64 issuedAt, bool reopen, Callback done) {
    const auto id = params["draft_id"].toString(params["conversation_id"].toString()), key = account + '\n' + id;
    const auto old = m_draftDiscardUi.value(key);
    if (!m_connected || (m_shutdown && !m_undoingDiscards && m_bulkDiscardTargets.isEmpty()) || id.isEmpty() || accountRemoved(account) || old.active
        || (!old.params.isEmpty() && (old.method != method || old.params != params))
        || (old.params.isEmpty() && m_draftDiscardUi.size() >= 64)) {
        done({{"message", "Resolve pending draft actions before continuing."}}, false); return;
    }
    DiscardUi ui; ui.method = method; ui.params = params; ui.active = true; ui.reopen = reopen; ui.issuedAt = issuedAt;
    ui.lifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    if (!old.params.isEmpty() && old.lifecycle != ui.lifecycle) { done({{"message", "The account changed after this draft action."}}, false); return; }
    m_draftDiscardUi[key] = ui; emit draftChanged();
    request(method, account, params, [this, method, account, params, id, key, ui, done = std::move(done)](auto result, bool ok) {
        if (!m_draftDiscardUi.contains(key) || m_draftDiscardUi[key].params != params) return;
        const bool valid = ok && result["account"] == account && result["draft_id"] == id && result["discarded"].isBool()
            && result["discarded"].toBool() == (method == "draft_discard")
            && result["revision"].toInteger(-1) == params["expected_revision"].toInteger() + 1
            && (method == "draft_discard" || result["undone_discard_request_id"] == params["discard_request_id"]);
        const bool same = account == m_account && id == m_draft.value("draft_id").toString()
            && ui.lifecycle == lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong() && !m_unsavedAccountBuffer;
        if (valid) {
            m_draftDiscardUi.remove(key);
            if (method == "draft_discard" && same && !olderDraftProjection(result)) {
                m_draft = mergedDraftProjection(result); restoreRecipientInputs(m_draft); m_draftDirty = false; m_draftConflict = false;
                m_draftStatus = "Draft discarded"; remember("replyOpen", false); remember("selectedDraft", "");
                emit presentationChanged();
            }
        } else {
            const bool refused = !ok && QStringList{"invalid_request", "invalid_draft", "stale_revision", "draft_read_only", "account_removed", "not_found", "scope_mismatch", "permission_denied"}.contains(result["code"].toString());
            if (refused) m_draftDiscardUi.remove(key);
            else { auto &pending = m_draftDiscardUi[key]; pending.active = false;
                pending.feedback = method == "draft_discard" ? "Discard could not be confirmed. Check its saved result before editing." : "Undo could not be confirmed. Retry the same Undo request."; }
            if (same) m_draftStatus = refused ? result["message"].toString("The draft action was refused; your version remains here.") : m_draftDiscardUi[key].feedback;
            cancelShutdown();
        }
        emit draftChanged(); done(result, valid);
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::offerDiscardUndo(const QVariantList &tokens, qint64 issuedAt) {
    if (tokens.isEmpty()) return;
    showActionToast(tokens.size() == 1 ? "Draft discarded" : QString::number(tokens.size()) + " drafts discarded",
        "undo_discard", {{"drafts", tokens}, {"issued_at", issuedAt}}, 6000, true);
}
void MailBackend::finishBulkDiscardUndo() {
    if (m_bulkDiscardUndo.isEmpty() && m_bulkDiscardTargets.isEmpty()) return;
    const auto tokens = m_bulkDiscardUndo; m_bulkDiscardUndo.clear(); m_bulkDiscardTargets.clear(); emit draftChanged();
    offerDiscardUndo(tokens, m_bulkIssuedAt);
}
void MailBackend::checkDraftDiscard() {
    if (!draftDiscard().value("can_check").toBool()) return;
    const auto key = googleSendKey(), account = m_account; const auto saved = m_draftDiscardUi[key];
    m_draftDiscardUi[key].active = true; emit draftChanged();
    request("draft_discard_status", account, {{"request_id", saved.params["request_id"]}}, [this, key, account, saved](auto result, bool ok) {
        if (!m_draftDiscardUi.contains(key) || m_draftDiscardUi[key].params != saved.params) return;
        m_draftDiscardUi[key].active = false;
        if (ok && result["account"] == account && result["request_id"] == saved.params["request_id"]) {
            const auto receipt = result["discard"].toObject();
            const auto token = discardUndoToken(account, receipt, saved.reopen);
            if (token.isEmpty()) { m_draftDiscardUi[key].feedback = "The discard result was incomplete. Check again."; emit draftChanged(); return; }
            m_draftDiscardUi.remove(key);
            if (account == m_account && receipt["draft_id"].toString() == m_draft.value("draft_id").toString() && !m_unsavedAccountBuffer
                && saved.lifecycle == lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) {
                m_draftDirty = false; m_recipientInputs.clear(); m_draftStatus = "Draft discarded"; remember("replyOpen", false);
                loadActiveDraft(); emit presentationChanged();
            }
            if (result["can_undo"].toBool()) offerDiscardUndo({token}, saved.issuedAt);
            else showActionToast("Discard confirmed. The draft has changed since.");
            loadAccounts(); if (m_folder == "drafts") loadDraftRows(false);
        } else {
            auto &pending = m_draftDiscardUi[key]; pending.notFound = !ok && result["code"] == "not_found";
            pending.feedback = pending.notFound ? "No saved discard was found. Retry the original request." : "The saved discard could not be checked. Try again after reconnecting.";
        }
        emit draftChanged();
    });
}
void MailBackend::retryDraftDiscard() {
    if (!draftDiscard().value("can_retry").toBool()) return;
    const auto saved = m_draftDiscardUi.value(googleSendKey()); const auto account = m_account;
    startDraftDiscardRequest(saved.method, account, saved.params, saved.issuedAt, saved.reopen, [this, account, saved](auto result, bool ok) {
        if (!ok) return;
        if (saved.method == "draft_discard") {
            const auto token = discardUndoToken(account, result, saved.reopen);
            if (!token.isEmpty()) offerDiscardUndo({token}, saved.issuedAt); else showActionToast("Draft discarded");
        }
        else { showActionToast("Draft restored", {}, {}, 2800, true); openDraftInAccount(account, result["draft_id"].toString()); }
        loadAccounts(); if (m_folder == "drafts") loadDraftRows(false);
    });
}
void MailBackend::undoDraftDiscards(const QVariantList &tokens) {
    if (m_undoingDiscards || tokens.isEmpty() || tokens.size() > 200 || !m_connected || m_shutdown) return;
    m_undoDiscardCommands.clear(); m_undoDiscardReopen.clear(); m_undoDiscardCompleted = 0; m_undoDiscardTotal = int(tokens.size());
    for (const auto &value : tokens) {
        const auto token = value.toMap(); const auto account = token.value("account").toString(), id = token.value("draft_id").toString();
        if (account.isEmpty() || id.isEmpty() || token.value("discard_request_id").toString().isEmpty()) { m_undoDiscardCommands.clear(); return; }
        m_undoDiscardCommands.append({{"account", account}, {"lifecycle", token.value("lifecycle").toLongLong()}, {"reopen", token.value("reopen").toBool()},
            {"params", QJsonObject{{"draft_id", id}, {"discard_request_id", token.value("discard_request_id").toString()}, {"expected_revision", token.value("revision").toLongLong()}, {"request_id", mutation()["request_id"]}}}});
    }
    m_undoingDiscards = true; emit draftChanged(); runDiscardUndo();
}
void MailBackend::runDiscardUndo() {
    if (m_undoDiscardCommands.isEmpty()) {
        m_undoingDiscards = false; emit draftChanged();
        showActionToast(m_undoDiscardCompleted == 1 ? "Draft restored" : QString::number(m_undoDiscardCompleted) + " drafts restored", {}, {}, 2800, true);
        if (m_shutdown) { saveDraft(); finishShutdown(); return; }
        loadAccounts(); if (m_folder == "drafts") loadDraftRows(false);
        if (m_afterDraftSave) saveDraft();
        else if (!m_undoDiscardReopen.isEmpty()) openDraftInAccount(m_undoDiscardReopen.value("account").toString(), m_undoDiscardReopen.value("draft_id").toString());
        else loadActiveDraft();
        if (m_shutdown) finishShutdown(); return;
    }
    const auto command = m_undoDiscardCommands.takeFirst(), params = command["params"].toObject();
    const auto account = command["account"].toString();
    auto done = [this, command, params, account](const QJsonObject &result, bool ok) {
        if (!ok) {
            m_undoDiscardCommands.clear(); m_undoingDiscards = false; emit draftChanged();
            const auto progress = m_undoDiscardTotal > 1 ? QString::number(m_undoDiscardCompleted) + " of " + QString::number(m_undoDiscardTotal) + " drafts confirmed restored. " : QString{};
            showActionToast(progress + "Undo needs attention. " + result["message"].toString("Open the draft to check it."), "open_draft", {{"account", account}, {"draft_id", params["draft_id"].toString()}}, 6000, true);
            loadAccounts(); if (m_folder == "drafts") loadDraftRows(false); cancelShutdown(); return;
        }
        ++m_undoDiscardCompleted;
        if (command["reopen"].toBool()) m_undoDiscardReopen = {{"account", account}, {"draft_id", params["draft_id"].toString()}};
        runDiscardUndo();
    };
    if (accountRemoved(account) || command["lifecycle"].toInteger() != lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) {
        done({{"message", "The account changed after this discard."}}, false); return;
    }
    startDraftDiscardRequest("draft_undo_discard", account, params, m_actionClock.elapsed(), command["reopen"].toBool(), done);
}
