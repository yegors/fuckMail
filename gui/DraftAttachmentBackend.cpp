#include "MailBackend.h"
#include <QDir>
#include <QJsonArray>
#include <QUrl>

namespace {
QString importKey(const QString &account, const QString &draft) { return account + '\n' + draft; }
}

QString MailBackend::attachmentImportKey() const {
    return importKey(m_account, m_draft.value("draft_id").toString());
}
bool MailBackend::attachmentImportPending() const {
    return m_attachmentImports.contains(attachmentImportKey());
}
QVariantMap MailBackend::attachmentImport() const {
    const auto key = attachmentImportKey();
    return {{"busy", m_attachmentImportActive == key}, {"pending", m_attachmentImports.contains(key)}, {"retry", m_attachmentImportRetry.contains(key)},
        {"status", m_attachmentImportStatus.value(key)}};
}

void MailBackend::importDraftAttachment(const QUrl &file, const QString &account, const QString &draftId) {
    // The native picker captures its origin before opening. A later selection
    // cannot attach to whichever draft happens to be displayed by then.
    if (account != m_account || draftId.isEmpty() || draftId != m_draft.value("draft_id").toString()) return;
    const auto key = importKey(account, draftId);
    auto reject = [this, key](const QString &message) { m_attachmentImportStatus[key] = message; emit draftChanged(); };
    if (!m_connected || m_shutdown || draftLocked()) { reject("Reconnect and finish the current draft operation before attaching a file."); return; }
    if (m_draftConflict) { reject("Resolve the saved draft conflict before attaching a file. Your edits remain here."); return; }
    if (attachmentImportBusy() || m_attachmentImports.contains(key)) { reject("Finish or retry the pending attachment before choosing another file."); return; }
    if (!file.isLocalFile() || !QDir::isAbsolutePath(file.toLocalFile())) { reject("Choose a local file using its absolute path."); return; }
    if (m_attachmentImports.size() >= 8) { reject("Resolve the pending attachment imports in your other drafts first."); return; }
    const bool needsSave = m_draft.value("revision").toLongLong() <= 0 || m_draft.value("discarded").toBool();
    if (m_draftDirty || draftBusy() || needsSave) {
        // A virtual or reopened discarded legacy reply first becomes a normal
        // saved draft, through the same revision/source checks as typed edits.
        if (needsSave) m_draftDirty = true;
        m_afterDraftSave = [this, file, account, draftId] { importDraftAttachment(file, account, draftId); };
        saveDraft(); return;
    }
    auto params = mutation();
    params["account"] = account; params["draft_id"] = draftId;
    params["expected_revision"] = m_draft.value("revision").toLongLong();
    params["path"] = file.toLocalFile();
    m_attachmentImports.insert(key, params);
    startDraftAttachmentImport(key);
}

void MailBackend::retryDraftAttachment() {
    if (draftReadOnly()) return;
    const auto key = attachmentImportKey();
    if (!m_attachmentImportRetry.contains(key) || attachmentImportBusy() || draftBusy() || !m_connected || m_shutdown) return;
    // Do not stat/reopen the path or regenerate the request/revision. Core
    // resolves a recorded receipt before touching a now-missing source file.
    startDraftAttachmentImport(key);
}

void MailBackend::startDraftAttachmentImport(const QString &key) {
    const auto intent = m_attachmentImports.value(key);
    if (intent.isEmpty() || attachmentImportBusy()) return;
    const auto account = intent["account"].toString();
    auto params = intent; params.remove("account");
    m_attachmentImportActive = key; m_attachmentImportRetry.remove(key);
    m_attachmentImportStatus[key] = "Attaching file…"; emit draftChanged();
    googleOperation("draft_attachment_import", account, params, [this, key, intent](auto result, bool ok) {
        m_attachmentImportActive.clear();
        const bool current = key == attachmentImportKey();
        const auto draft = result["draft"].toObject();
        const auto attachment = result["attachment"].toObject();
        if (ok && (draft["account"] != intent["account"] || draft["draft_id"] != intent["draft_id"]
            || attachment["account"] != intent["account"] || attachment["id"].toString().isEmpty()
            || !draft["attachment_ids"].toArray().contains(attachment["id"])
            || draft["revision"].toInteger() != intent["expected_revision"].toInteger() + 1)) {
            result = QJsonObject{{"code", "invalid_response"}, {"message", "Attachment result did not match the original draft. Retry to check its saved result."}};
            ok = false;
        }
        if (!ok) {
            const auto code = result["code"].toString();
            const bool uncertain = code == "timeout" || code == "engine_disconnected" || code == "cancelled"
                || code == "invalid_response" || code == "busy" || code == "engine_untrusted";
            if (uncertain) m_attachmentImportRetry.insert(key);
            else { m_attachmentImports.remove(key); m_attachmentImportRetry.remove(key); }
            m_attachmentImportStatus[key] = (uncertain ? "Attachment result not confirmed — " : "Not attached — ")
                + result["message"].toString("the import failed. Your edits remain here.");
            if (current && code == "stale_revision") {
                m_draftConflict = true; m_draftStatus = "Not saved — another interface changed this draft. Your version remains here.";
            }
            if (current && !(m_shutdown && !m_draftDirty)) {
                cancelShutdown(); fail(m_attachmentImportStatus[key]);
            }
            emit draftChanged();
            if (current && !uncertain && !m_draftConflict) saveDraft();
            if (m_shutdown) finishShutdown();
            return;
        }
        m_attachmentImports.remove(key); m_attachmentImportRetry.remove(key);
        m_attachmentImportStatus[key] = "Attached " + attachment["name"].toString() + ".";
        if (current && m_draft.value("revision").toLongLong() == intent["expected_revision"].toInteger()) {
            // Preserve all newer local authored fields and attachment removals.
            // Only this import's new selection and durable revision are merged.
            auto ids = m_draft.value("attachment_ids").toList();
            if (!ids.contains(attachment["id"].toString())) ids.append(attachment["id"].toString());
            QVariantList files;
            for (const auto &value : draft["attachments"].toArray())
                if (ids.contains(value.toObject()["id"].toString())) files.append(value.toVariant());
            m_draft["attachment_ids"] = ids; m_draft["attachments"] = files;
            m_draft["revision"] = draft["revision"].toVariant();
            m_draft["updated_at"] = draft["updated_at"].toVariant();
            m_draftDirty = m_draftDirty || ids != draft["attachment_ids"].toArray().toVariantList();
        }
        // A newer projection or another account/draft is never replaced by the
        // original receipt, including a replay returned after navigation.
        if (current) mergeGoogleDraftStatus(draft["google"].toObject(), intent["account"].toString(), intent["draft_id"].toString());
        if (current) mergeGoogleSendStatus(draft["google_send"].toObject(), intent["account"].toString(), intent["draft_id"].toString());
        emit draftChanged();
        if (current) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}
