#include "MailBackend.h"
#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QRegularExpression>
#include <QUrl>

QVariantMap MailBackend::googleDraftInventory() const {
    auto result = m_draftInventoryAccount == m_account ? m_draftInventory : QVariantMap{};
    const bool current = m_draftInventoryAccount == m_account;
    result["loading"] = current && m_draftInventoryLoading;
    result["refreshing"] = current && m_draftInventoryRefreshing;
    result["can_refresh"] = !m_syntheticProfile && m_connected && !m_shutdown
        && accountCapability(m_account, "mail_read") && googleSyncState(m_account).value("owned").toBool()
        && !(current && m_draftInventoryRefreshing);
    const auto sync = googleSyncState(m_account);
    const auto progress = sync.value("progress").toMap();
    const auto phase = progress.value("remote_drafts").toString();
    const bool active = sync.value("active").toBool();
    result["worker_state"] = phase; result["worker_active"] = active;
    QString text;
    if (current && !m_draftInventoryFeedback.isEmpty()) text = m_draftInventoryFeedback;
    else if (active && phase == "paused")
        text = "Google draft discovery paused: " + progress.value("remote_drafts_error").toString();
    else if (result.value("paused").isValid() && !result.value("paused").toString().isEmpty())
        text = "Google draft discovery paused: " + result.value("paused").toString();
    else if (result.value("generation").toLongLong() == 0)
        text = "Cached drafts · start Google sync in Settings to discover changes";
    else if (result.value("enumeration_complete").toBool())
        text = QString("Last check: %1 observed · %2 cached sources").arg(result.value("observed_this_scan").toLongLong()).arg(result.value("source_retained").toLongLong());
    else if (active && (phase == "running" || phase == "starting"))
        text = QString("Checking Google drafts · %1 observed · %2 pending").arg(result.value("observed_this_scan").toLongLong()).arg(result.value("pending_items").toLongLong());
    else text = QString("Cached incomplete check · %1 observed · %2 pending%3").arg(result.value("observed_this_scan").toLongLong())
        .arg(result.value("pending_items").toLongLong()).arg(active ? " · waiting" : " · sync stopped");
    if (result.value("source_unavailable").toLongLong() > 0)
        text += QString(" · %1 source unavailable").arg(result.value("source_unavailable").toLongLong());
    result["status"] = text;
    return result;
}

void MailBackend::loadGoogleDraftInventory() {
    if (m_syntheticProfile || !m_connected || m_shutdown || m_account.isEmpty() || m_folder != "drafts") return;
    if (m_draftInventoryAccount != m_account) {
        m_draftInventoryAccount = m_account; m_draftInventory.clear(); m_draftInventoryFeedback.clear();
        ++m_draftInventoryGeneration; m_draftInventoryLoading = false; m_draftInventoryRefreshing = false;
    }
    if (m_draftInventoryLoading || m_draftInventoryRefreshing) return;
    const auto account = m_account;
    const int generation = m_draftInventoryGeneration;
    m_draftInventoryLoading = true;
    request("google_drafts_status", account, {}, [this, account, generation](auto result, bool ok) {
        if (account != m_account || account != m_draftInventoryAccount || generation != m_draftInventoryGeneration) return;
        m_draftInventoryLoading = false;
        if (ok && result["account"] == account) {
            const auto next = result.toVariantMap();
            if (next.value("generation").toLongLong() > m_draftInventory.value("generation").toLongLong()
                || (next.value("generation") == m_draftInventory.value("generation")
                    && next.value("revision").toLongLong() >= m_draftInventory.value("revision").toLongLong())
                || m_draftInventory.isEmpty()) m_draftInventory = next;
        } else m_draftInventoryFeedback = "Cached Google draft status is unavailable.";
        emit googleDraftInventoryChanged();
    });
}
void MailBackend::refreshGoogleDraftInventory() {
    if (!googleDraftInventory().value("can_refresh").toBool() || m_folder != "drafts") return;
    if (m_draftInventoryAccount != m_account) {
        m_draftInventoryAccount = m_account; m_draftInventory.clear(); m_draftInventoryLoading = false;
    }
    const auto account = m_account;
    const int generation = ++m_draftInventoryGeneration;
    m_draftInventoryRefreshing = true; m_draftInventoryFeedback.clear(); emit googleDraftInventoryChanged();
    googleOperation("google_drafts_refresh", account, {}, [this, account, generation](auto result, bool ok) {
        if (account != m_account || account != m_draftInventoryAccount || generation != m_draftInventoryGeneration) return;
        m_draftInventoryRefreshing = false; m_draftInventoryLoading = false;
        if (ok && result["account"] == account) m_draftInventory = result.toVariantMap();
        else m_draftInventoryFeedback = result["message"].toString("Google draft refresh could not start.");
        emit googleDraftInventoryChanged();
        // This is a cached projection refresh; provider work has its explicit owner.
        if (ok && m_folder == "drafts") loadDraftRows(false, true);
    });
}

QVariantList MailBackend::draftSnapshotHeaders() const {
    if (!draftReadOnly() && !accountArchived() && !m_unsavedAccountBuffer) return {};
    const auto headers = m_draft.value("remote_source").toMap().value("raw_headers").toMap();
    QVariantList rows;
    for (const auto &entry : {qMakePair("from", "From"), qMakePair("to", "To"), qMakePair("cc", "Cc"),
             qMakePair("bcc", "Bcc"), qMakePair("reply_to", "Reply-To")}) {
        auto text = headers.value(entry.first).toString();
        if (QString::fromLatin1(entry.first) == "from"
            && (text.isEmpty() || m_draft.value("remote_source").toMap().value("source_state").toString() != "available")) {
            const auto name = m_draft.value("from_name").toString(), address = m_draft.value("from").toString();
            text = name.isEmpty() ? address : name + " <" + address + ">";
        }
        // Raw header previews can be truncated. Show every parsed recipient,
        // retaining the raw header as fallback when parsing was unavailable.
        const QString field = QString::fromLatin1(entry.first);
        if (field == "to" || field == "cc" || field == "bcc") {
            QStringList addresses;
            for (const auto &value : m_draft.value(field + "_recipients").toList()) {
                const auto recipient = value.toMap();
                const auto name = recipient.value("name").toString(), email = recipient.value("email").toString();
                addresses.append(name.isEmpty() ? email : name + " <" + email + ">");
            }
            if (!addresses.isEmpty()) text = addresses.join("\n");
            const auto pending = m_recipientInputs.value(field + "_recipients");
            if (!pending.isEmpty()) text += (text.isEmpty() ? QString{} : "\n") + pending + " (not saved)";
        }
        if (!text.isEmpty() || QString::fromLatin1(entry.first) == "from" || QString::fromLatin1(entry.first) == "to")
            rows.append(QVariantMap{{"label", entry.second}, {"value", text}});
    }
    return rows;
}
QString MailBackend::draftSourcesKey() const {
    if (m_draft.value("remote_source").toMap().value("source_revision").toLongLong() <= 0) return {};
    return m_account + '\n' + m_draft.value("draft_id").toString() + '\n'
        + QString::number(m_draft.value("remote_source").toMap().value("source_revision").toLongLong());
}
QVariantMap MailBackend::draftSources() const {
    if (m_draftSourcesKey != draftSourcesKey() || m_draftSourcesKey.isEmpty()) return {};
    auto result = m_draftSources;
    result["loading"] = m_draftSourcesLoading; result["exporting"] = m_draftSourceExporting;
    result["status"] = m_draftSourceFeedback; result["export"] = m_draftSourceExport;
    return result;
}
void MailBackend::resetDraftSourcesContext() {
    const auto key = draftSourcesKey();
    if (key == m_draftSourcesKey) return;
    m_draftSourcesKey = key; ++m_draftSourcesGeneration;
    ++m_draftSourceExportGeneration;
    m_draftSources.clear(); m_draftSourceExport.clear(); m_draftSourceFeedback.clear();
    m_draftSourcesLoading = false; m_draftSourceExporting = false;
}
void MailBackend::loadDraftSources(bool append) {
    if (draftSourcesKey().isEmpty() || !m_connected || m_shutdown) return;
    resetDraftSourcesContext();
    if (m_draftSourcesLoading) return;
    auto next = m_draftSources.value("next_offset");
    if (!next.isValid() || next.isNull()) next = m_draftSources.value("delete_next_offset");
    if (append && (!next.isValid() || next.isNull())) return;
    const int offset = append ? next.toInt() : 0;
    if (offset < 0) return;
    const auto key = m_draftSourcesKey, account = m_account, id = m_draft.value("draft_id").toString();
    const int generation = ++m_draftSourcesGeneration;
    m_draftSourcesLoading = true; emit draftSourcesChanged();
    request("google_draft_sources", account, {{"draft_id", id}, {"limit", 20}, {"offset", offset}},
        [this, key, account, id, generation, offset](auto result, bool ok) {
            if (key != draftSourcesKey() || generation != m_draftSourcesGeneration) return;
            m_draftSourcesLoading = false;
            if (ok && result["account"] == account && result["draft_id"] == id) {
                QVariantList rows;
                for (const auto &entry : result["sources"].toArray()) {
                    const auto row = entry.toObject().toVariantMap();
                    bool present = false;
                    for (const auto &existing : rows) if (existing.toMap().value("source_revision") == row.value("source_revision")) { present = true; break; }
                    if (!present) rows.append(row);
                }
                m_draftSources = result.toVariantMap(); m_draftSources["sources"] = rows; m_draftSources["offset"] = offset;
            } else m_draftSourceFeedback = result["message"].toString("Saved source history is unavailable.");
            emit draftSourcesChanged();
        });
}
void MailBackend::exportDraftSource(qint64 sourceRevision, const QString &attachmentId) {
    if (draftSourcesKey().isEmpty() || !m_connected || m_shutdown || sourceRevision <= 0
        || sourceRevision > m_draft.value("remote_source").toMap().value("source_revision").toLongLong()) return;
    if (sourceRevision == m_draft.value("remote_source").toMap().value("source_revision").toLongLong()
        && m_draft.value("remote_source").toMap().value("source_state").toString() != "available") return;
    resetDraftSourcesContext();
    if (m_draftSourceExporting) return;
    const auto exportGeneration = ++m_draftSourceExportGeneration;
    const auto key = m_draftSourcesKey, account = m_account, id = m_draft.value("draft_id").toString();
    QJsonObject params{{"draft_id", id}, {"source_revision", sourceRevision}};
    if (!attachmentId.isEmpty()) params["attachment_id"] = attachmentId;
    m_draftSourceExporting = true; m_draftSourceFeedback = "Preparing a verified local copy…"; emit draftSourcesChanged();
    googleOperation("google_draft_source", account, params, [this, key, account, id, sourceRevision, attachmentId, exportGeneration](auto result, bool ok) {
        if (key != draftSourcesKey() || key != m_draftSourcesKey || exportGeneration != m_draftSourceExportGeneration) return;
        m_draftSourceExporting = false;
        static const QRegularExpression hash("^[0-9a-f]{64}$");
        if (ok && (result["account"] != account || result["draft_id"] != id
            || result["source_revision"].toInteger() != sourceRevision
            || result["attachment_id"].toString() != attachmentId || !hash.match(result["sha256"].toString()).hasMatch()
            || result["size"].toInteger(-1) < 0 || !QDir::isAbsolutePath(result["path"].toString()))) {
            ok = false; result = {{"message", "The exported copy did not match the requested saved source."}};
        }
        if (ok) { m_draftSourceExport = result.toVariantMap(); m_draftSourceFeedback = "Verified copy saved: " + result["name"].toString(); }
        else { m_draftSourceExport.clear(); m_draftSourceFeedback = result["message"].toString("The saved source could not be exported."); }
        emit draftSourcesChanged();
    });
}
void MailBackend::showDraftSourceFolder() {
    const auto path = draftSources().value("export").toMap().value("path").toString();
    if (path.isEmpty() || !QDir::isAbsolutePath(path)) return;
    if (!QDesktopServices::openUrl(QUrl::fromLocalFile(QFileInfo(path).absolutePath())))
        fail("The folder containing the verified copy could not be opened.");
}
