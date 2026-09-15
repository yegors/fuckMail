#include "MailBackend.h"

namespace {
QString transferKey(const QString &account, const QString &draft) { return account + '\n' + draft; }
}

bool MailBackend::draftTransferPending() const {
    return m_draftTransferIntents.contains(transferKey(m_account, m_draft.value("draft_id").toString()));
}
QVariantMap MailBackend::draftTransfer() const {
    const auto key = transferKey(m_account, m_draft.value("draft_id").toString());
    const auto transfer = m_draft.value("account_transfer").toMap();
    const auto state = transfer.value("state").toString();
    auto message = m_draftTransferMessages.value(key);
    if (message.isEmpty() && state == "blocked") message = transfer.value("error").toMap().value("message").toString();
    if (message.isEmpty() && (state == "waiting_target" || state == "cleanup_pending"))
        message = "Account changed. The original Google draft stays until the new account finishes syncing.";
    if (m_draft.value("transferred").toBool()) message = "This draft was moved to another account. Its original content is retained here.";
    return {{"status", message}, {"busy", m_draftTransferActive == key || m_draftTransferCleanupBusy},
        {"retry", draftTransferPending() && m_draftTransferActive != key}, {"pending", draftTransferPending()},
        {"cleanup_retry", state == "blocked"}, {"transferred", m_draft.value("transferred").toBool()},
        {"can_change", !draftLocked() && !draftBusy() && !attachmentImportPending()
            && (state.isEmpty() || state == "complete")}};
}

void MailBackend::changeDraftAccount(const QString &target) {
    if (target == m_account || lifecycleAccount(target).isEmpty() || accountRemoved(target)) return;
    const auto key = transferKey(m_account, m_draft.value("draft_id").toString());
    auto reject = [this, key](const QString &message) { m_draftTransferMessages[key] = message; emit draftChanged(); };
    if (!m_connected || m_shutdown || draftLocked() || attachmentImportPending() || m_draftConflict) {
        reject("Finish the current draft operation before changing From."); return;
    }
    if (!commitRecipientInputs()) return;
    if (m_draftDirty || draftBusy() || m_draft.value("revision").toLongLong() < 1) {
        const auto account = m_account, context = draftContext();
        m_afterDraftSave = [this, account, context, target] {
            if (m_account == account && draftContext() == context) changeDraftAccount(target);
        };
        if (m_draft.value("revision").toLongLong() < 1) m_draftDirty = true;
        saveDraft(); return;
    }
    cancelPendingGoogleSend("Send cancelled by From account change.");
    auto intent = mutation();
    intent["account"] = m_account; intent["draft_id"] = m_draft.value("draft_id").toString();
    intent["target_account"] = target; intent["expected_revision"] = m_draft.value("revision").toLongLong();
    intent["expected_google_revision"] = googleDraft().value("revision").toLongLong();
    intent["keep_original"] = false;
    m_draftTransferIntents[key] = intent;
    startDraftTransfer(key);
}

void MailBackend::retryDraftTransfer() {
    if (!m_draftTransferActive.isEmpty() || !m_connected || m_shutdown || accountArchived() || m_unsavedAccountBuffer) return;
    startDraftTransfer(transferKey(m_account, m_draft.value("draft_id").toString()));
}

void MailBackend::startDraftTransfer(const QString &key) {
    const auto intent = m_draftTransferIntents.value(key);
    if (intent.isEmpty() || !m_draftTransferActive.isEmpty()) return;
    const auto source = intent["account"].toString(), target = intent["target_account"].toString();
    const auto sourceLife = lifecycleAccount(source).value("lifecycle").toMap().value("revision");
    const auto targetLife = lifecycleAccount(target).value("lifecycle").toMap().value("revision");
    const int generation = m_generation;
    auto params = intent; params.remove("account");
    m_draftTimer.stop(); m_draftTransferActive = key; m_draftTransferMessages[key] = "Changing From account…"; emit draftChanged();
    googleOperation("draft_transfer", source, params, [this, key, intent, sourceLife, targetLife, generation](auto result, bool ok) {
        m_draftTransferActive.clear();
        const auto source = intent["account"].toString(), target = intent["target_account"].toString();
        const auto draft = result["draft"].toObject();
        if (ok && (result["account"] != intent["account"] || result["request_id"] != intent["request_id"]
            || result["source_draft_id"] != intent["draft_id"] || result["target_account"] != intent["target_account"]
            || result["target_draft_id"].toString().isEmpty() || draft["draft_id"] != result["target_draft_id"]
            || draft["account"] != intent["target_account"])) {
            ok = false; result = {{"code", "invalid_response"}, {"message", "The account change result was incomplete. Check it again."}};
        }
        const bool current = source == m_account && intent["draft_id"].toString() == m_draft.value("draft_id").toString()
            && generation == m_generation && !accountRemoved(source) && !accountRemoved(target) && !m_unsavedAccountBuffer
            && sourceLife == lifecycleAccount(source).value("lifecycle").toMap().value("revision")
            && targetLife == lifecycleAccount(target).value("lifecycle").toMap().value("revision");
        if (!ok) {
            const auto code = result["code"].toString();
            const bool definite = code == "stale_revision" || code == "stale_google_revision" || code == "invalid_request"
                || code == "invalid_draft" || code == "draft_read_only" || code == "draft_transfer_pending"
                || code == "google_draft_pending" || code == "google_send_pending" || code == "limit_exceeded"
                || code == "account_removed" || code == "not_found";
            if (definite) m_draftTransferIntents.remove(key);
            m_draftTransferMessages[key] = (definite ? "Account unchanged. " : "Account change not confirmed. ")
                + result["message"].toString("Check the saved result before editing this draft.");
            m_afterDraftSave = {}; cancelShutdown(); emit draftChanged();
            if (current && definite) loadActiveDraft();
            return;
        }
        m_draftTransferIntents.remove(key); m_draftTransferMessages.remove(key);
        if (current) {
            m_draft["discarded"] = true; m_draft["transferred"] = true; m_draft["read_only"] = true;
            m_draft["account_transfer"] = result.toVariantMap(); m_draftDirty = false;
            remember("selectedDraft", QString{}); remember("replyOpen", false);
            if (m_afterDraftSave) { auto callback = std::move(m_afterDraftSave); m_afterDraftSave = {}; callback(); }
            else openDraftInAccount(target, result["target_draft_id"].toString());
        }
        emit draftChanged(); loadAccounts();
        if (m_folder == "drafts") loadDraftRows(false);
        if (m_shutdown) finishShutdown();
    });
}

void MailBackend::openDraftInAccount(const QString &account, const QString &draft) {
    if (account.isEmpty() || draft.isEmpty() || accountRemoved(account)) return;
    if (account == m_account) { openDraft(draft); return; }
    m_draftTransferDestination = {{"account", account}, {"draft_id", draft}};
    selectAccount(account);
}
void MailBackend::openDraftTransferDestination() {
    const auto transfer = m_draft.value("account_transfer").toMap();
    openDraftInAccount(transfer.value("target_account").toString(), transfer.value("target_draft_id").toString());
}
void MailBackend::retryDraftTransferCleanup() {
    const auto transfer = m_draft.value("account_transfer").toMap();
    if (m_draftTransferCleanupBusy || transfer.value("state") != "blocked" || !m_connected || m_shutdown) return;
    const auto source = transfer.value("account").toString(), account = m_account, id = m_activeDraft;
    auto params = mutation(); params["transfer_request_id"] = transfer.value("request_id").toString();
    m_draftTransferCleanupBusy = true; emit draftChanged();
    request("draft_transfer_retry", source, params, [this, account, id](auto result, bool ok) {
        m_draftTransferCleanupBusy = false;
        if (account != m_account || id != m_activeDraft) return;
        if (ok) { m_draft["account_transfer"] = result.toVariantMap(); m_draftTransferMessages.remove(transferKey(account,id)); }
        else m_draftTransferMessages[transferKey(account,id)] = result["message"].toString("The account change could not be checked.");
        emit draftChanged();
    });
}
