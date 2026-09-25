#include "MailBackend.h"
#include <QJsonArray>
#include <QUuid>

QVariantMap MailBackend::lifecycleAccount(const QString &account) const {
    for (const auto &value : m_allAccounts) if (value.toMap().value("id").toString() == account) return value.toMap();
    for (const auto &value : m_accounts) if (value.toMap().value("id").toString() == account) return value.toMap();
    return {};
}
bool MailBackend::accountRemoved(const QString &account) const {
    return lifecycleAccount(account).value("lifecycle").toMap().value("state").toString() == "removed";
}
bool MailBackend::accountArchived() const { return accountRemoved(m_account); }
QVariantMap MailBackend::accountLifecycle() const {
    return {{"operations", m_accountLifecycleUi}, {"unsaved_email", m_unsavedAccountEmail},
        {"unsaved_account", m_unsavedAccountBuffer ? m_account : QString{}},
        {"unsaved_draft", m_unsavedAccountBuffer ? m_draft.value("draft_id") : QVariant{}}};
}

bool MailBackend::archivedRequestAllowed(const QString &method) const {
    // Only cached queries/verified exports and the explicit lifecycle workflow.
    // In particular, provider reconciliation and local abandon are mutations.
    static const QSet<QString> cached{
        "accounts", "view_get", "categories", "conversations", "conversation", "message_source", "message_invite", "drafts", "draft_get",
        "sender_summary", "attachment", "capture_history", "draft_capture", "google_drafts_status",
        "google_draft_sources", "google_draft_source", "google_draft_status", "google_send_status", "google_sent", "google_sent_message",
        "google_draft_delete_status", "google_mail_mutation_status", "calendar_operations", "google_calendar_operation",
        "calendar_combined", "calendar_sources", "calendar", "account_remove_preview", "account_remove",
        "account_remove_status", "account_archive_delete", "google_auth_restore_begin"};
    return cached.contains(method);
}
bool MailBackend::blockArchivedRequest(const QString &method, const QString &account) const {
    return !account.isEmpty() && accountRemoved(account) && !archivedRequestAllowed(method);
}

bool MailBackend::accountNavigationAllowed() {
    if (!m_unsavedAccountBuffer) return true;
    emit accountBufferDispositionRequired(); return false;
}
void MailBackend::freezeRemovedAccountBuffer() {
    // Keep every authored field and incomplete recipient under the old identity.
    m_unsavedAccountBuffer = true;
    m_unsavedAccountEmail = m_draft.value("from").toString();
    if (m_unsavedAccountEmail.isEmpty()) m_unsavedAccountEmail = lifecycleAccount(m_account).value("email").toString();
    m_draftTimer.stop(); ++m_draftSaveGeneration; ++m_generation; ++m_listGeneration;
    m_savingDraft = false; m_resolvingDraft = false;
    m_afterDraftSave = {}; m_afterConversationMutation = {};
    cancelPendingGoogleSend();
    m_draftStatus = "Unsaved edits retained here — this account was removed. They are not part of the saved archive.";
    cancelShutdown(); emit draftChanged(); emit accountLifecycleChanged();
}
void MailBackend::resolveUnsavedAccountBuffer(bool discard) {
    if (!m_unsavedAccountBuffer) return;
    if (!discard) { m_afterAccountDisposition = {}; return; }
    // Explicitly discards only this process's unsaved buffer, never a Core draft.
    m_unsavedAccountBuffer = false; m_unsavedAccountEmail.clear(); m_draftDirty = false;
    m_draftConflict = false; m_recipientInputs.clear(); m_draft.clear(); m_activeDraft.clear();
    m_selected.clear(); m_conversation.clear(); m_draftStatus.clear(); ++m_generation;
    emit accountLifecycleChanged(); emit draftChanged(); emit conversationChanged();
    auto continuation = std::move(m_afterAccountDisposition); m_afterAccountDisposition = {};
    if (continuation) continuation();
    else if (!m_accounts.isEmpty()) { const auto id = m_accounts.first().toMap().value("id").toString(); clearAccountSelection(); selectAccount(id); }
    else clearAccountSelection();
}
void MailBackend::clearAccountSelection() {
    if (m_unsavedAccountBuffer) return;
    ++m_generation; ++m_listGeneration; ++m_draftSaveGeneration;
    m_account.clear(); m_archiveSelection.clear(); m_selected.clear(); m_activeDraft.clear();
    m_conversation.clear(); m_draft.clear(); m_recipientInputs.clear(); m_draftStatus.clear();
    m_draftDirty = false; m_draftConflict = false; m_savingDraft = false; m_resolvingDraft = false;
    m_query.clear(); m_category.clear(); m_filters.clear(); m_querySummary.clear(); m_folder = "inbox";
    m_pinnedRow.clear(); m_pinnedIndex = -1; m_collapsedGroups.clear(); m_categories.clear(); m_captures.clear();
    m_captureNextOffset = -1; m_nextOffset = -1; m_nextCursor.clear(); m_loading = false;
    m_afterDraftSave = {}; m_afterConversationMutation = {}; clearSenderCard(); publishConversations({});
    emit accountChanged(); emit currentAccountChanged(); emit accountLifecycleChanged(); emit draftChanged();
    emit conversationChanged(); emit conversationsChanged(); emit categoriesChanged(); emit queryChanged(); emit capturesChanged();
}
void MailBackend::reconcileAccountLifecycle() {
    const auto row = lifecycleAccount(m_account);
    if (!m_account.isEmpty() && row.isEmpty()) {
        cancelGoogleProviderOperations(m_account);
        if (!m_unsavedAccountBuffer && (m_draftDirty || m_savingDraft || !m_recipientInputs.isEmpty())) freezeRemovedAccountBuffer();
        else if (!m_unsavedAccountBuffer) {
            clearAccountSelection();
            if (!m_accounts.isEmpty()) selectAccount(m_accounts.first().toMap().value("id").toString());
        }
    }
    const auto lifecycle = row.value("lifecycle").toMap();
    const auto revision = lifecycle.value("revision").toLongLong();
    const auto previous = m_accountLifecycleRevisions.value(m_account);
    if (!m_account.isEmpty() && previous > 0 && revision > previous && !accountArchived()) {
        // A complete remove/restore may happen between polls. Its monotonic
        // epoch still invalidates every old completion and unsent continuation.
        if (!m_unsavedAccountBuffer && (m_draftDirty || m_savingDraft || !m_recipientInputs.isEmpty())) freezeRemovedAccountBuffer();
        else if (!m_unsavedAccountBuffer) {
            const auto account = m_account; clearAccountSelection(); selectAccount(account);
        }
    }
    if (!m_account.isEmpty() && accountArchived() && (previous != revision || m_archiveSelection != m_account)) {
        m_draftTimer.stop(); m_afterDraftSave = {}; m_afterConversationMutation = {}; cancelPendingGoogleSend();
        m_viewDirty.remove(m_account); m_viewPending.remove(m_account);
        cancelGoogleProviderOperations(m_account);
        if (!m_unsavedAccountBuffer && (m_draftDirty || m_savingDraft || !m_recipientInputs.isEmpty())) freezeRemovedAccountBuffer();
        else if (!m_unsavedAccountBuffer && m_archiveSelection != m_account) {
            clearAccountSelection();
            if (!m_accounts.isEmpty()) selectAccount(m_accounts.first().toMap().value("id").toString());
        }
    }
    for (const auto &entry : m_allAccounts) {
        const auto value = entry.toMap();
        const auto id = value.value("id").toString();
        const auto life = value.value("lifecycle").toMap();
        if (life.value("state") == "active" && life.value("revision").toLongLong() > m_accountLifecycleRevisions.value(id)
            && !m_accountRemovalBusy.contains(id)) m_accountLifecycleUi.remove(id);
        m_accountLifecycleRevisions[id] = life.value("revision").toLongLong();
    }
    emit accountLifecycleChanged();
    if (previous != revision) emit draftChanged();
}

void MailBackend::previewAccountRemoval(const QString &account) {
    if (!m_connected || m_shutdown || m_syntheticProfile || account.isEmpty() || accountRemoved(account)
        || m_accountRemovalBusy.contains(account) || m_unsavedAccountBuffer) return;
    if (m_accountLifecycleUi.value(account).toMap().value("uncertain").toBool()) {
        fail("Check or retry the original removal request before preparing another one."); return;
    }
    // A passive saved-view response may still select its stored draft and
    // advance the editor generation. Retain this explicit target until that
    // response has established the editor; a different user navigation still
    // invalidates the captured view generation.
    if (m_accountViewRestorationGeneration == m_generation) {
        m_deferredAccountRemovalPreview = {{"account", account}, {"view_account", m_account},
            {"view_generation", m_generation}, {"lifecycle_revision", lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()}};
        return;
    }
    if (account == m_account && (m_draftDirty || draftBusy() || !m_recipientInputs.isEmpty())) {
        m_afterDraftSave = [this, account] { previewAccountRemoval(account); }; saveDraft(); return;
    }
    const auto generation = m_generation;
    request("account_remove_preview", account, {}, [this, account, generation](auto result, bool ok) {
        if (generation != m_generation || m_unsavedAccountBuffer || accountRemoved(account)) {
            fail("The account or editor changed. Review removal again."); return;
        }
        if (!ok) { fail(result["message"].toString("Account removal could not be prepared.")); return; }
        const auto lifecycle = result["lifecycle"].toObject();
        if (result["account"] != account || lifecycle["state"] != "active" || lifecycle["revision"].toInteger() <= 0) {
            fail("The account removal preview had an unexpected identity."); return;
        }
        m_accountRemovalConfirmation = result;
        m_accountRemovalConfirmation["email"] = lifecycleAccount(account).value("email").toString();
        m_accountRemovalConfirmation["ui_generation"] = generation;
        m_accountRemovalConfirmation["request_id"] = "gui-remove-" + QUuid::createUuid().toString(QUuid::WithoutBraces);
        emit accountRemovalReady(m_accountRemovalConfirmation.toVariantMap());
    });
}
void MailBackend::resumeAccountRemovalAfterView(const QString &account, int generation, bool restored) {
    if (m_deferredAccountRemovalPreview.value("view_account") != account
        || m_deferredAccountRemovalPreview.value("view_generation").toInt(-1) != generation) return;
    const auto intent = std::move(m_deferredAccountRemovalPreview); m_deferredAccountRemovalPreview = {};
    const auto target = intent["account"].toString();
    if (!restored || account != m_account || m_unsavedAccountBuffer || accountRemoved(target)
        || intent["lifecycle_revision"].toInteger() != lifecycleAccount(target).value("lifecycle").toMap().value("revision").toLongLong()) {
        fail("The account or editor changed. Review removal again."); return;
    }
    // openDraft may now be resolving; the existing save continuation waits for
    // its completion before obtaining a fresh removal confirmation.
    previewAccountRemoval(target);
}
void MailBackend::confirmAccountRemoval(const QVariantMap &context) {
    const auto captured = QJsonObject::fromVariantMap(context);
    const auto account = captured["account"].toString();
    if (captured.isEmpty() || captured != m_accountRemovalConfirmation || captured["ui_generation"].toInt() != m_generation
        || m_unsavedAccountBuffer || accountRemoved(account) || (account == m_account && (m_draftDirty || draftBusy()))) {
        fail("The account or editor changed. Review removal again."); return;
    }
    const auto revision = captured["lifecycle"].toObject()["revision"].toInteger();
    if (revision != lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) {
        fail("The account changed. Review removal again."); return;
    }
    m_accountRemovalConfirmation = {};
    submitAccountRemoval(account, {{"expected_revision", revision}, {"request_id", captured["request_id"]}, {"retention", "archive"}});
}
void MailBackend::submitAccountRemoval(const QString &account, const QJsonObject &params) {
    if (!m_connected || m_shutdown || m_accountRemovalBusy.contains(account)) return;
    m_accountRemovalRequests[account] = params; m_accountRemovalBusy.insert(account); ++m_accountWrites;
    m_accountLifecycleUi[account] = QVariantMap{{"busy", true}, {"request_id", params["request_id"].toString()}, {"status", "Removing from this app and cleaning up local sign-in credentials…"}};
    emit accountLifecycleChanged();
    request("account_remove", account, params, [this, account, params](auto result, bool ok) {
        m_accountRemovalBusy.remove(account); --m_accountWrites;
        auto ui = m_accountLifecycleUi.value(account).toMap(); ui["busy"] = false;
        ui["status"] = ok ? "Account removed. Saved mail, drafts and history are kept in its archive."
                          : result["message"].toString("Removal result not confirmed. Check its saved status before retrying.");
        ui["uncertain"] = !ok; if (ok) ui["result"] = result.toVariantMap();
        m_accountLifecycleUi[account] = ui; emit accountLifecycleChanged();
        loadAccounts(); loadCalendarSources(); loadCalendar();
        if (!ok) checkAccountRemoval(account, params["request_id"].toString());
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::checkAccountRemoval(const QString &account, const QString &requestId) {
    if (!m_connected || m_shutdown || requestId.isEmpty()) return;
    request("account_remove_status", account, {{"request_id", requestId}}, [this, account, requestId](auto result, bool ok) {
        auto ui = m_accountLifecycleUi.value(account).toMap();
        if (ui.value("request_id").toString() != requestId) return;
        if (ok && result["account"] == account && result["request_id"] == requestId) {
            ui["result"] = result.toVariantMap(); ui["uncertain"] = false;
            ui["status"] = "Saved removal receipt available. Current cleanup is shown separately.";
            m_accountLifecycleUi[account] = ui; emit accountLifecycleChanged(); loadAccounts();
        }
    });
}
void MailBackend::retryAccountCleanup(const QString &account) {
    if (m_accountLifecycleUi.value(account).toMap().value("uncertain").toBool()
        && m_accountRemovalRequests.contains(account)) {
        submitAccountRemoval(account, m_accountRemovalRequests.value(account)); return;
    }
    if (!accountRemoved(account) || m_accountRemovalBusy.contains(account)) return;
    const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap();
    const auto cleanup = lifecycle.value("cleanup").toMap();
    const auto state = cleanup.value("state").toString();
    if (state == "in_flight" || state == "restore_in_flight" || state == "complete") return;
    auto params = m_accountRemovalRequests.value(account);
    if (state != "pending" || !cleanup.value("replay_cleanup").toBool()
        || params.value("request_id").toString() != cleanup.value("request_id").toString())
        params = {{"expected_revision", lifecycle.value("revision").toLongLong()},
            {"request_id", "gui-remove-" + QUuid::createUuid().toString(QUuid::WithoutBraces)}, {"retention", "archive"}};
    submitAccountRemoval(account, params);
}
void MailBackend::openAccountArchive(const QString &account) {
    if (!accountRemoved(account)) return;
    if (m_unsavedAccountBuffer) { m_afterAccountDisposition = [this, account] { openAccountArchive(account); }; accountNavigationAllowed(); return; }
    m_archiveSelection = account; selectAccount(account);
}
void MailBackend::previewArchiveDeletion(const QString &account) {
    if (!m_connected || m_shutdown || !accountRemoved(account) || m_unsavedAccountBuffer || m_accountRemovalBusy.contains(account)) return;
    const auto row = lifecycleAccount(account);
    const auto life = row.value("lifecycle").toMap();
    if (life.value("cleanup").toMap().value("state") != "complete") return;
    if (m_archiveDeletionConfirmation["account"] != account || m_archiveDeletionConfirmation["expected_revision"].toInteger() != life.value("revision").toLongLong())
    m_archiveDeletionConfirmation = {{"account", account}, {"email", row.value("email").toString()},
        {"expected_revision", life.value("revision").toLongLong()},
        {"request_id", "gui-purge-" + QUuid::createUuid().toString(QUuid::WithoutBraces)}};
    emit archiveDeletionReady(m_archiveDeletionConfirmation.toVariantMap());
}
void MailBackend::confirmArchiveDeletion(const QVariantMap &context) {
    const auto captured = QJsonObject::fromVariantMap(context);
    const auto account = captured["account"].toString();
    if (captured.isEmpty() || captured != m_archiveDeletionConfirmation || !accountRemoved(account)
        || m_unsavedAccountBuffer || m_accountRemovalBusy.contains(account) || !m_connected || m_shutdown) return;
    m_accountRemovalBusy.insert(account); ++m_accountWrites;
    m_accountLifecycleUi[account] = QVariantMap{{"busy", true}, {"status", "Deleting local archive…"}};
    emit accountLifecycleChanged();
    request("account_archive_delete", account, {{"expected_revision", captured["expected_revision"]}, {"request_id", captured["request_id"]}},
        [this, account](auto result, bool ok) {
            m_accountRemovalBusy.remove(account); --m_accountWrites;
            if (ok) { m_accountLifecycleUi.remove(account); m_archiveDeletionConfirmation = {}; }
            else m_accountLifecycleUi[account] = QVariantMap{{"busy", false}, {"status", result["message"].toString("Archive deletion could not be confirmed. Try again.")}};
            emit accountLifecycleChanged(); loadAccounts(); loadCalendarSources(); loadCalendar();
            if (m_shutdown) finishShutdown();
        });
}
void MailBackend::restoreGoogleAccount(const QString &email, const QString &account, qint64 revision, bool reauthorize, const QVariantMap &access) {
    const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap();
    if (!accountRemoved(account) || lifecycle.value("revision").toLongLong() != revision
        || lifecycle.value("cleanup").toMap().value("state").toString() != "complete") {
        fail("Restoration needs the current archived account and completed credential cleanup."); return;
    }
    beginGoogleAuth("google_auth_restore_begin", email, account, reauthorize, revision, access);
}
