#include "MailBackend.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QTimeZone>
#include <algorithm>

QJsonObject MailBackend::draftSelector() const {
    return m_activeDraft.isEmpty() ? QJsonObject{{"conversation_id", m_selected}}
                                   : QJsonObject{{"draft_id", m_activeDraft}};
}
QString MailBackend::draftContext() const {
    return m_activeDraft.isEmpty() ? "conversation:" + m_selected : "draft:" + m_activeDraft;
}

bool MailBackend::olderDraftProjection(const QJsonObject &result) const {
    // Cached queries and attachment imports have independent connections. A
    // late read must not move a known draft revision backwards after a receipt.
    return result["account"].toString() == m_account
        && !result["draft_id"].toString().isEmpty()
        && result["draft_id"].toString() == m_draft.value("draft_id").toString()
        && result["revision"].toInteger() < m_draft.value("revision").toLongLong();
}

void MailBackend::acceptDraft(const QJsonObject &result, bool restoreView) {
    if (olderDraftProjection(result)) {
        m_draftStatus = m_draftDirty ? "Saving…" : "Saved on this machine";
        emit draftChanged(); return;
    }
    m_recipientInputs.clear();
    m_activeDraft = result["draft_id"].toString();
    m_selected = result["conversation_id"].toString();
    m_draft = mergedDraftProjection(result); m_draftDirty = false; m_draftConflict = false;
    m_draftStatus = "Saved on this machine";
    m_conversation.clear(); m_pinnedRow.clear(); m_pinnedIndex = -1;
    remember("selectedDraft", m_activeDraft); remember("selected", m_selected);
    if (!restoreView) { remember("readerY", 0); remember("replyOpen", true); }
    emit conversationChanged(); emit draftChanged();
    if (!m_selected.isEmpty()) loadConversation(false);
    else emit ready();
}

void MailBackend::beginDraft(const QString &mode) {
    beginDraftWithRecipient(mode, {}, {});
}
void MailBackend::writeToSender(const QString &email, const QString &name) {
    const auto address = email.trimmed();
    if (address.isEmpty() || !validFilterSender(address)) return;
    beginDraftWithRecipient("new", address, name);
}
void MailBackend::beginDraftWithRecipient(const QString &mode, const QString &email, const QString &name) {
    if (!m_connected || m_account.isEmpty()) return;
    if (mode != "new" && mode != "reply" && mode != "reply_all" && mode != "forward") return;
    if (m_draftDirty || draftBusy()) {
        const auto account = m_account;
        m_afterDraftSave = [this, account, mode, email, name] { if (account == m_account) beginDraftWithRecipient(mode, email, name); };
        saveDraft(); return;
    }
    const auto members = m_conversation.value("messages").toList();
    if (mode != "new" && members.isEmpty()) return;
    const auto source = mode == "new" ? QString{} : members.last().toMap()["id"].toString();
    if (m_draftCreation.isEmpty() || m_draftCreationAccount != m_account
        || m_draftCreation["mode"].toString() != mode || m_draftCreation["source_message_id"].toString() != source || m_draftCreationEmail != email) {
        m_draftCreation = mutation(); m_draftCreation["mode"] = mode;
        if (mode != "new") { m_draftCreation["conversation_id"] = m_selected; m_draftCreation["source_message_id"] = source; }
        m_draftCreationAccount = m_account;
        m_draftCreationEmail = email;
    }
    // Retain the operation identity on a lost response, so retry cannot create
    // another copy of this draft. The Core journal is the durable authority.
    m_resolvingDraft = true; m_draftStatus = "Opening draft…"; emit draftChanged();
    const auto account = m_account;
    request("draft_create", account, m_draftCreation, [this, account, email, name](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account) return;
        if (ok) {
            ++m_generation; m_draftCreation = {}; m_draftCreationEmail.clear(); acceptDraft(result);
            if (!email.isEmpty()) {
                const bool safeName = name.toUtf8().size() <= 4096 && std::none_of(name.begin(), name.end(), [](QChar c) { return c.category() == QChar::Other_Control; });
                // The card supplies a structured Core mailbox. Preserve it
                // directly; the draft save validates it without reparsing a
                // formatted recipient string or losing quoted/Unicode names.
                m_draft["to_recipients"] = QVariantList{QVariantMap{{"email", email}, {"name", safeName ? name : QString{}}}};
                m_draftDirty = true; m_draftStatus = "Saving…";
                clearError(); emit draftChanged();
            }
            loadAccounts(); if (m_folder == "drafts") loadDraftRows(false);
        }
        else { m_draftStatus = "Could not open the draft. Retry keeps the same draft operation."; cancelShutdown(); emit draftChanged(); }
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}

void MailBackend::openDraft(const QString &draftId, bool restoreView) {
    if (draftId.isEmpty() || !m_connected) return;
    if (m_draftDirty || draftBusy()) { m_afterDraftSave = [this, draftId, restoreView] { openDraft(draftId, restoreView); }; saveDraft(); return; }
    m_resolvingDraft = true; m_draftStatus = "Opening draft…"; emit draftChanged();
    const auto account = m_account;
    const int generation = ++m_generation;
    request("draft_get", account, {{"draft_id", draftId}}, [this, account, generation, restoreView](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account || generation != m_generation) return;
        if (ok && !result["discarded"].toBool()) acceptDraft(result, restoreView);
        else { m_draftStatus = "This draft is unavailable. Your current editor is unchanged."; cancelShutdown(); emit draftChanged(); }
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}

void MailBackend::loadActiveDraft() {
    if (m_activeDraft.isEmpty() || (m_syntheticProfile && (m_draftDirty || draftBusy()))) return;
    const auto account = m_account, id = m_activeDraft;
    const int generation = m_generation;
    request("draft_get", account, {{"draft_id", id}}, [this, account, id, generation](auto result, bool ok) {
        if (!ok || account != m_account || id != m_activeDraft || generation != m_generation
            || result["account"].toString() != account || result["draft_id"].toString() != id) return;
        const bool metadataChanged = mergeGoogleDraftStatus(result["google"].toObject(), account, id);
        if (m_draftDirty || draftBusy() || olderDraftProjection(result)) { if (metadataChanged) emit draftChanged(); return; }
        const auto next = mergedDraftProjection(result);
        if (next != m_draft) {
            m_draft = next; m_draftStatus = result["discarded"].toBool() ? "Draft discarded" : "Saved on this machine";
            emit draftChanged();
        }
        else if (metadataChanged) emit draftChanged();
    });
}

void MailBackend::loadDraftRows(bool append, bool preserveLoaded) {
    const auto account = m_account;
    const int generation = ++m_listGeneration, offset = append ? m_nextOffset : 0;
    if (offset < 0) return;
    m_loading = true; emit loadingChanged();
    const int desired = append ? int(m_conversations.size()) + 50 : preserveLoaded ? std::max({50, int(m_conversations.size()), m_desiredConversationCount}) : 50;
    m_desiredConversationCount = desired;
    loadDraftPage(account, generation, offset, append ? m_conversations : QVariantList{}, desired);
}
void MailBackend::loadDraftPage(const QString &account, int generation, int offset, QVariantList rows, int desiredCount) {
    request("drafts", account, {{"query", m_query}, {"limit", 50}, {"offset", offset}},
        [this, account, generation, offset, rows = std::move(rows), desiredCount](auto result, bool ok) mutable {
            if (account != m_account || generation != m_listGeneration || m_folder != "drafts") return;
            if (ok) {
                for (const auto &value : result["drafts"].toArray()) {
                    auto row = value.toObject().toVariantMap();
                    row["id"] = row["draft_id"]; row["is_draft"] = true; row["group"] = "drafts";
                    row["subject"] = row["subject"].toString().isEmpty() ? "(No subject)" : row["subject"];
                    QStringList recipients;
                    for (const auto &entry : row["to_recipients"].toList()) {
                        const auto address = entry.toMap();
                        recipients.append(address.value("name").toString().isEmpty() ? address["email"].toString() : address["name"].toString());
                    }
                    row["sender_name"] = recipients.isEmpty() ? "No recipients" : "To " + recipients.join(", ");
                    row["latest_at"] = QDateTime::fromSecsSinceEpoch(row["updated_at"].toLongLong(), QTimeZone::UTC).toString(Qt::ISODate);
                    row["categories"] = QVariantList{}; row["unread"] = false; row["starred"] = false; row["message_count"] = 1;
                    auto existing = std::find_if(rows.begin(), rows.end(), [&row](const QVariant &value) { return value.toMap()["draft_id"] == row["draft_id"]; });
                    if (existing == rows.end()) rows.append(row); else *existing = row;
                }
                const int next = result["next_offset"].isDouble() ? result["next_offset"].toInt() : -1;
                if (!result["drafts"].toArray().isEmpty() && next > offset && rows.size() < desiredCount) {
                    loadDraftPage(account, generation, next, std::move(rows), desiredCount); return;
                }
                if (!publishConversations(std::move(rows))) { m_loading = false; emit loadingChanged(); return; }
                m_querySummary = {{"filtered_conversation_count", result["total"].toVariant()}};
                m_nextOffset = next;
                m_nextCursor.clear(); emit conversationsChanged();
            }
            m_loading = false; emit loadingChanged();
        });
}

bool MailBackend::addDraftRecipient(const QString &field, const QString &text) {
    if (draftLocked() || m_draft.isEmpty() || (field != "to_recipients" && field != "cc_recipients" && field != "bcc_recipients")) return false;
    const auto input = text.trimmed();
    static const QRegularExpression addressPattern(R"(^(?:(.*?)\s*<([^<>\s,;]+@[^<>\s,;]+)>|([^<>\s,;]+@[^<>\s,;]+))$)");
    const auto match = addressPattern.match(input);
    const bool control = std::any_of(input.begin(), input.end(), [](QChar ch) { return ch.category() == QChar::Other_Control; });
    if (!match.hasMatch() || control) {
        fail("Enter an email address, optionally as Name <address@example.com>."); return false;
    }
    QString name = match.captured(1).trimmed(), email = match.captured(2);
    if (email.isEmpty()) email = match.captured(3);
    if (name.startsWith('"') && name.endsWith('"') && name.size() > 1) name = name.mid(1, name.size() - 2);
    static const QRegularExpression mailboxPattern(R"(^[A-Za-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[A-Za-z0-9!#$%&'*+/=?^_`{|}~-]+)*@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$)");
    if (email.toUtf8().size() > 320 || name.toUtf8().size() > 4096 || !mailboxPattern.match(email).hasMatch()) {
        fail("Enter a valid mailbox address (up to 320 UTF-8 bytes) and a display name up to 4096 bytes."); return false;
    }
    auto entries = m_draft[field].toList();
    for (const auto &entry : entries) if (entry.toMap()["email"].toString().compare(email, Qt::CaseInsensitive) == 0) { m_recipientInputs.remove(field); return true; }
    qsizetype count = 0;
    QVariantMap routing{{"subject", m_draft["subject"]}};
    for (const auto &key : {"to_recipients", "cc_recipients", "bcc_recipients"}) { count += m_draft[key].toList().size(); routing[key] = m_draft[key]; }
    if (count >= 100) { fail("A draft supports at most 100 recipients in total."); return false; }
    entries.append(QVariantMap{{"name", name}, {"email", email}});
    routing[field] = entries;
    // Leave room for the bounded frozen source references in Core's 64 KiB envelope.
    if (QJsonDocument::fromVariant(routing).toJson(QJsonDocument::Compact).size() > 48 * 1024) { fail("Recipient names and addresses exceed this draft's local size limit."); return false; }
    m_recipientInputs.remove(field); m_draft[field] = entries; m_draftDirty = true; m_draftStatus = "Saving…";
    clearError(); emit draftChanged(); m_draftTimer.start(); return true;
}
void MailBackend::stageDraftRecipient(const QString &field, const QString &text) {
    if (draftLocked() || m_draft.isEmpty() || (field != "to_recipients" && field != "cc_recipients" && field != "bcc_recipients")) return;
    if (text.trimmed().isEmpty()) m_recipientInputs.remove(field);
    else { m_recipientInputs[field] = text; m_draftDirty = true; }
}
bool MailBackend::commitRecipientInputs() {
    const auto inputs = m_recipientInputs;
    for (auto it = inputs.cbegin(); it != inputs.cend(); ++it) {
        if (!addDraftRecipient(it.key(), it.value())) {
            m_draftStatus = "Not saved — finish or remove the incomplete recipient. Your text remains here.";
            cancelShutdown(); emit draftChanged(); return false;
        }
    }
    return true;
}
void MailBackend::removeDraftRecipient(const QString &field, int index) {
    if (draftLocked() || (field != "to_recipients" && field != "cc_recipients" && field != "bcc_recipients")) return;
    auto entries = m_draft[field].toList();
    if (index < 0 || index >= entries.size()) return;
    entries.removeAt(index); m_draft[field] = entries; m_draftDirty = true; m_draftStatus = "Saving…";
    emit draftChanged(); m_draftTimer.start();
}
void MailBackend::setDraftSubject(const QString &subject) {
    if (draftLocked() || m_draft.isEmpty() || m_draft["subject"].toString() == subject) return;
    m_draft["subject"] = subject; m_draftDirty = true; m_draftStatus = "Saving…";
    emit draftChanged(); m_draftTimer.start();
}
