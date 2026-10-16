// MailBackend's draft composition: starting and opening drafts, recipient chips,
// validation and suggestions, the subject, the Drafts list and chips row, and
// discarding a new message that was left empty.
#include "MailBackend.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QTimeZone>
#include <QUuid>
#include <algorithm>

QJsonObject MailBackend::draftSelector() const {
    return m_activeDraft.isEmpty() ? QJsonObject{{"conversation_id", m_selected}}
                                   : QJsonObject{{"draft_id", m_activeDraft}};
}
QString MailBackend::draftContext() const {
    const auto identity = m_activeDraft.isEmpty() ? "conversation:" + m_selected : "draft:" + m_activeDraft;
    return identity + "/lifecycle:" + QString::number(lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong());
}

bool MailBackend::olderDraftProjection(const QJsonObject &result) const {
    // Cached queries and attachment imports have independent connections. A
    // late read must not move a known draft revision backwards after a receipt.
    const bool same = result["account"].toString() == m_account
        && !result["draft_id"].toString().isEmpty()
        && result["draft_id"].toString() == m_draft.value("draft_id").toString();
    if (!same) return false;
    // Imported observations have their own source clock; an unchanged authored
    // revision does not make an earlier cached source safe to publish.
    if (draftReadOnly() && !m_draft.value("transferred").toBool()) {
        const auto incoming = result["remote_source"].toObject();
        const auto current = m_draft.value("remote_source").toMap();
        const auto scan = incoming["latest_scan_generation"].toInteger(), oldScan = current.value("latest_scan_generation").toLongLong();
        return result["origin"].toString() != "google_snapshot" || !result["read_only"].toBool()
            || incoming["source_revision"].toInteger() < current.value("source_revision").toLongLong()
            || scan < oldScan || incoming["seen_generation"].toInteger() < current.value("seen_generation").toLongLong()
            || (scan == oldScan && current.value("latest_scan_complete").toBool() && !incoming["latest_scan_complete"].toBool());
    }
    return result["revision"].toInteger() < m_draft.value("revision").toLongLong();
}

void MailBackend::acceptDraft(const QJsonObject &result, bool restoreView) {
    if (m_unsavedAccountBuffer) return;
    if (olderDraftProjection(result)) {
        if (!draftReadOnly()) m_draftStatus = m_draftDirty ? "Saving…" : "Saved on this machine";
        emit draftChanged(); return;
    }
    if (result["draft_id"].toString() != m_draft.value("draft_id").toString() && !m_attachmentQueues.value(attachmentImportKey()).isEmpty())
        m_attachmentQueuePaused.insert(attachmentImportKey());
    // A draft for the thread on screen keeps the thread there while it
    // reloads, rather than blanking it and starting from the top.
    const bool sameThread = !m_selected.isEmpty() && result["conversation_id"].toString() == m_selected && !m_conversation.isEmpty();
    restoreRecipientInputs(result.toVariantMap());
    m_activeDraft = result["draft_id"].toString();
    m_selected = result["conversation_id"].toString();
    m_draft = mergedDraftProjection(result); m_draftDirty = false; m_draftConflict = false;
    m_draftStatus = draftDiscardPending() ? "Draft action needs attention" : m_draft.value("transferred").toBool() ? "Draft moved to another account" : draftReadOnly() ? "Read-only Google draft preview" : "Saved on this machine";
    if (!sameThread) { m_conversation.clear(); m_pinnedRow.clear(); m_pinnedIndex = -1; }
    remember("selectedDraft", m_activeDraft); remember("selected", m_selected);
    if (!restoreView) { if (!sameThread) remember("readerY", 0); remember("replyOpen", true); }
    emit conversationChanged(); emit draftChanged();
    if (!restoreView) emit presentationChanged();
    if (!m_selected.isEmpty()) loadConversation(false);
    else emit ready();
}

void MailBackend::beginDraft(const QString &mode) {
    beginDraftWithRecipient(mode, {}, {});
}
void MailBackend::beginDraftFrom(const QString &mode, const QString &messageId) {
    if (mode == "new" || messageId.isEmpty()) return;
    beginDraftWithRecipient(mode, {}, {}, false, messageId);
}
QString MailBackend::threadReplySource() const {
    static const QRegularExpression forwarded(QStringLiteral(R"(^\s*fwd?\s*:)"), QRegularExpression::CaseInsensitiveOption);
    const auto members = m_conversation.value("messages").toList();
    for (auto it = members.crbegin(); it != members.crend(); ++it) {
        const auto member = it->toMap();
        if (!member.value("sent").toBool() || !forwarded.match(member.value("subject").toString()).hasMatch()) return member.value("id").toString();
    }
    return members.isEmpty() ? QString{} : members.last().toMap().value("id").toString();
}
void MailBackend::writeToSender(const QString &email, const QString &name) {
    const auto address = email.trimmed();
    if (address.isEmpty() || !validFilterSender(address)) return;
    beginDraftWithRecipient("new", address, name);
}
void MailBackend::switchDraft(const QString &mode) {
    if (mode != "new") beginDraftWithRecipient(mode, {}, {}, true);
}
void MailBackend::beginDraftWithRecipient(const QString &mode, const QString &email, const QString &name, bool carry, const QString &sourceId) {
    if (!m_connected || m_account.isEmpty() || accountArchived() || m_unsavedAccountBuffer) return;
    if (mode != "new" && mode != "reply" && mode != "reply_all" && mode != "forward") return;
    // A second press while that same kind is opening changes nothing.
    if (m_resolvingDraft && m_draftCreation["mode"].toString() == mode) return;
    cancelPendingGoogleSend("Send cancelled by draft navigation.");
    // Save pending edits first; the continuation re-runs this navigation if
    // the same account is still selected.
    if (m_draftDirty || draftBusy()) {
        const auto account = m_account;
        m_afterDraftSave = [this, account, mode, email, name, carry, sourceId] { if (account == m_account) beginDraftWithRecipient(mode, email, name, carry, sourceId); };
        saveDraft(); return;
    }
    const auto members = m_conversation.value("messages").toList();
    if (mode != "new" && (members.isEmpty() || m_conversation.value("remote").toBool())) return;
    if (!sourceId.isEmpty() && std::none_of(members.cbegin(), members.cend(), [&](const auto &member) { return member.toMap().value("id") == sourceId; })) return;
    if (mode == "new" && m_draft.value("mode").toString() != "new") m_newDraftOrigin = {m_account, m_selected, m_draft.value("discarded").toBool() ? QString{} : m_activeDraft};
    // Turning this thread's reply into a forward, or back, takes the words
    // written so far into the new draft and retires the old one once the new
    // one is saved. A draft with files the writer added, one being sent, or
    // one with words anywhere but above its starting text stays as it is.
    QString carried; QJsonObject retired;
    const auto currentMode = m_draft.value("mode").toString();
    if (carry && mode != "new" && !m_selected.isEmpty() && m_draft.value("conversation_id").toString() == m_selected
        && (currentMode.isEmpty() ? QStringLiteral("reply") : currentMode) != mode && m_draft.value("revision").toLongLong() > 0
        && !m_draft.value("discarded").toBool() && !draftReadOnly() && !googleSendBlocksDraftSync()
        && !draftAddedFiles() && !draftDiscardPending()) {
        bool untouched = false;
        carried = authoredDraftText(&untouched);
        if (untouched) { retired = draftSelector(); retired["expected_revision"] = m_draft.value("revision").toLongLong(); }
    }
    // Switching between Reply and Forward keeps the message being answered.
    const auto kept = carry && m_draft.value("conversation_id").toString() == m_selected && (currentMode.isEmpty() ? QStringLiteral("reply") : currentMode) != mode
        && !m_draft.value("discarded").toBool() ? m_draft.value("source_message_id").toString() : QString{};
    dropEmptyNewDraft();
    const auto source = mode == "new" ? QString{} : !sourceId.isEmpty() ? sourceId : !kept.isEmpty() ? kept : threadReplySource();
    if (m_draftCreation.isEmpty() || m_draftCreationAccount != m_account
        || m_draftCreation["mode"].toString() != mode || m_draftCreation["source_message_id"].toString() != source || m_draftCreationEmail != email) {
        m_draftCreation = mutation(); m_draftCreation["mode"] = mode;
        if (mode != "new") { m_draftCreation["conversation_id"] = m_selected; m_draftCreation["source_message_id"] = source; }
        m_draftCreationAccount = m_account;
        m_draftCreationEmail = email;
    }
    // Retain the operation identity on a lost response, so retry cannot create
    // another copy of this draft. The Core journal is the durable authority.
    if (!m_attachmentQueues.value(attachmentImportKey()).isEmpty()) m_attachmentQueuePaused.insert(attachmentImportKey());
    m_resolvingDraft = true; m_draftStatus = "Opening draft…"; emit draftChanged();
    const auto account = m_account;
    request("draft_create", account, m_draftCreation, [this, account, email, name, carried, retired](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account || accountRemoved(account) || m_unsavedAccountBuffer) return;
        if (ok) {
            ++m_generation; m_draftCreation = {}; m_draftCreationEmail.clear(); acceptDraft(result);
            if (!carried.isEmpty()) {
                const auto start = m_draft.value("body").toString();
                m_draft["body"] = carried + (start.isEmpty() || start.startsWith('\n') ? QString{} : QStringLiteral("\n\n")) + start;
                m_draft["body_html"] = QVariant(); m_draftDirty = true; m_draftStatus = "Saving…"; emit draftChanged();
            }
            if (!retired.isEmpty()) m_afterDraftSave = [this, account, retired] { retireDraft(account, retired); };
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
    if (m_unsavedAccountBuffer) { m_afterAccountDisposition = [this, draftId, restoreView] { openDraft(draftId, restoreView); }; accountNavigationAllowed(); return; }
    if (draftId.isEmpty() || !m_connected) return;
    cancelPendingGoogleSend("Send cancelled by draft navigation.");
    if (m_draftDirty || draftBusy()) { m_afterDraftSave = [this, draftId, restoreView] { openDraft(draftId, restoreView); }; saveDraft(); return; }
    if (draftId != m_activeDraft) dropEmptyNewDraft();
    m_resolvingDraft = true; m_draftStatus = "Opening draft…"; emit draftChanged();
    const auto account = m_account;
    const int generation = ++m_generation;
    request("draft_get", account, {{"draft_id", draftId}}, [this, account, draftId, generation, restoreView](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account || generation != m_generation) return;
        if (ok && (!result["discarded"].toBool() || result["transferred"].toBool() || m_draftDiscardUi.contains(account + '\n' + draftId))) acceptDraft(result, restoreView);
        else { m_draftStatus = "This draft is unavailable. Your current editor is unchanged."; cancelShutdown(); emit draftChanged(); }
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}

void MailBackend::loadActiveDraft() {
    if (m_unsavedAccountBuffer) return;
    if (m_activeDraft.isEmpty() || (m_syntheticProfile && (m_draftDirty || draftBusy()))) return;
    const auto account = m_account, id = m_activeDraft;
    const int generation = m_generation;
    request("draft_get", account, {{"draft_id", id}}, [this, account, id, generation](auto result, bool ok) {
        if (!ok || account != m_account || id != m_activeDraft || generation != m_generation
            || result["account"].toString() != account || result["draft_id"].toString() != id) return;
        const bool sendChanged = mergeGoogleSendStatus(result["google_send"].toObject(), account, id);
        const bool deletionChanged = mergeDraftDeletion(result["google_delete"].toObject(), account, id);
        const bool metadataChanged = mergeGoogleDraftStatus(result["google"].toObject(), account, id) || sendChanged || deletionChanged;
        if (m_draftDirty || draftBusy() || olderDraftProjection(result)) { if (metadataChanged) emit draftChanged(); return; }
        const auto next = mergedDraftProjection(result);
        if (next != m_draft) {
            m_draft = next; restoreRecipientInputs(m_draft); m_draftStatus = draftDiscardPending() ? "Draft action needs attention" : m_draft.value("transferred").toBool() ? "Draft moved to another account" : draftReadOnly() ? "Read-only Google draft preview" : result["discarded"].toBool() ? "Draft discarded" : "Saved on this machine";
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
            if (!ok && offset == 0 && result["code"].toString() == "invalid_request" && !m_query.trimmed().isEmpty()) {
                readSearchResult(result, false); m_loading = false; emit loadingChanged(); return;
            }
            if (ok && offset == 0) readSearchResult(result, true);
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
                    // An unknown save time stays blank rather than dating the draft to 1970.
                    const auto updated = row["updated_at"].toLongLong();
                    row["latest_at"] = updated > 0 ? QDateTime::fromSecsSinceEpoch(updated, QTimeZone::UTC).toString(Qt::ISODate) : QString();
                    row["categories"] = QVariantList{}; row["unread"] = false; row["starred"] = false; row["message_count"] = 1;
                    auto existing = std::find_if(rows.begin(), rows.end(), [&row](const QVariant &value) { return value.toMap()["draft_id"] == row["draft_id"]; });
                    if (existing == rows.end()) rows.append(row); else *existing = row;
                }
                // Keep paging until the list is as long as before, so a refresh
                // does not shrink what the user had scrolled through.
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
        return rejectRecipient("Enter an email address, optionally as Name <address@example.com>.");
    }
    QString name = match.captured(1).trimmed(), email = match.captured(2);
    if (email.isEmpty()) email = match.captured(3);
    if (name.startsWith('"') && name.endsWith('"') && name.size() > 1) name = name.mid(1, name.size() - 2);
    return addDraftAddress(field, email, name);
}
bool MailBackend::addDraftAddress(const QString &field, const QString &email, const QString &name) {
    if (draftLocked() || m_draft.isEmpty() || (field != "to_recipients" && field != "cc_recipients" && field != "bcc_recipients")) return false;
    if (std::any_of(name.begin(), name.end(), [](QChar ch) { return ch.category() == QChar::Other_Control; })) return false;
    static const QRegularExpression mailboxPattern(R"(^[A-Za-z0-9!#$%&'*+/=?^_`{|}~-]+(?:\.[A-Za-z0-9!#$%&'*+/=?^_`{|}~-]+)*@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)*$)");
    if (email.toUtf8().size() > 320 || name.toUtf8().size() > 4096 || !mailboxPattern.match(email).hasMatch()) {
        return rejectRecipient("Enter a valid mailbox address (up to 320 UTF-8 bytes) and a display name up to 4096 bytes.");
    }
    auto entries = m_draft[field].toList();
    for (const auto &entry : entries) if (entry.toMap()["email"].toString().compare(email, Qt::CaseInsensitive) == 0) { m_recipientInputs.remove(field); m_recipientError.clear(); emit draftChanged(); return true; }
    qsizetype count = 0;
    QVariantMap routing{{"subject", m_draft["subject"]}};
    for (const auto &key : {"to_recipients", "cc_recipients", "bcc_recipients"}) { count += m_draft[key].toList().size(); routing[key] = m_draft[key]; }
    if (count >= 100) return rejectRecipient("A draft supports at most 100 recipients in total.");
    entries.append(QVariantMap{{"name", name}, {"email", email}});
    routing[field] = entries;
    // Leave room for the bounded frozen source references in Core's 64 KiB envelope.
    if (QJsonDocument::fromVariant(routing).toJson(QJsonDocument::Compact).size() > 48 * 1024) return rejectRecipient("Recipient names and addresses exceed this draft's local size limit.");
    m_recipientInputs.remove(field); m_draft[field] = entries; m_draftDirty = true; m_draftStatus = "Saving…"; m_recipientError.clear();
    clearError(); emit draftChanged(); m_draftTimer.start(); return true;
}
// The composer shows this beside the recipient fields; the top error bar is gone.
bool MailBackend::rejectRecipient(const QString &message) {
    m_recipientError = message; emit draftChanged(); fail(message); return false;
}
QString MailBackend::recipientSuggestions(const QString &query, const QStringList &exclude) {
    const auto ticket = QUuid::createUuid().toString(QUuid::WithoutBraces);
    const auto account = m_account;
    // Dispatch from the event loop so even an immediate reply reaches QML
    // after it has stored the ticket.
    QTimer::singleShot(0, this, [this, ticket, account, query, exclude] {
        request("recipient_suggestions", account, {{"query", query}, {"exclude", QJsonArray::fromStringList(exclude)}},
            [this, ticket](auto result, bool ok) { emit recipientSuggestionsReply(ticket, result.toVariantMap(), ok); });
    });
    return ticket;
}
void MailBackend::stageDraftRecipient(const QString &field, const QString &text) {
    if (draftLocked() || m_draft.isEmpty() || (field != "to_recipients" && field != "cc_recipients" && field != "bcc_recipients")) return;
    const auto value = text.trimmed().isEmpty() ? QString{} : text;
    if (m_recipientInputs.value(field) == value) return;
    if (value.isEmpty()) m_recipientInputs.remove(field); else m_recipientInputs[field] = value;
    m_recipientError.clear(); m_draftDirty = true; m_draftStatus = "Saving…"; emit draftChanged(); m_draftTimer.start();
}
void MailBackend::restoreRecipientInputs(const QVariantMap &draft) {
    m_recipientInputs.clear(); m_recipientError.clear();
    const auto inputs = draft.value("recipient_inputs").toMap();
    for (auto it = inputs.cbegin(); it != inputs.cend(); ++it) m_recipientInputs[it.key()] = it.value().toString();
}
bool MailBackend::commitRecipientInputs() {
    if (draftReadOnly()) return m_recipientInputs.isEmpty();
    const auto inputs = m_recipientInputs;
    for (auto it = inputs.cbegin(); it != inputs.cend(); ++it) {
        if (!addDraftRecipient(it.key(), it.value())) {
            m_draftStatus = "Finish or remove the incomplete recipient before continuing. Your text remains here.";
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
// A subject is one line, and Core refuses control characters in it. The field
// draws a pasted line break or tab as a space, so each run of whitespace
// holding one becomes one space, or nothing at either end. Typed spaces stay.
static bool subjectBreak(QChar ch) {
    const auto category = ch.category();
    return category == QChar::Other_Control || category == QChar::Separator_Line || category == QChar::Separator_Paragraph;
}
static QString oneLineSubject(const QString &text) {
    QString out; out.reserve(text.size());
    for (qsizetype i = 0; i < text.size();) {
        if (!text[i].isSpace() && !subjectBreak(text[i])) { out += text[i++]; continue; }
        const auto start = i; bool broken = false;
        for (; i < text.size() && (text[i].isSpace() || subjectBreak(text[i])); ++i) broken = broken || subjectBreak(text[i]);
        if (!broken) out += text.mid(start, i - start);
        else if (!out.isEmpty() && i < text.size()) out += ' ';
    }
    return out;
}
void MailBackend::setDraftSubject(const QString &text) {
    const auto subject = oneLineSubject(text);
    if (draftLocked() || m_draft.isEmpty()) return;
    // Show the folded subject even when it matches what is already saved.
    if (m_draft["subject"].toString() == subject) { if (subject != text) emit draftChanged(); return; }
    m_draft["subject"] = subject; m_draftDirty = true; m_draftStatus = "Saving…";
    emit draftChanged(); m_draftTimer.start();
}

bool MailBackend::draftHasContent() const {
    if (m_draft.isEmpty()) return false;
    for (const auto field : {"to_recipients", "cc_recipients", "bcc_recipients", "attachment_ids"})
        if (!m_draft.value(field).toList().isEmpty()) return true;
    for (const auto &input : m_recipientInputs) if (!input.trimmed().isEmpty()) return true;
    // The starting signature alone is not something written.
    const auto body = m_draft.value("body").toString().trimmed();
    return !m_draft.value("subject").toString().trimmed().isEmpty()
        || (!body.isEmpty() && body != currentAccount().value("signature_text").toString().trimmed());
}
bool MailBackend::authoredDraftRow(const QVariantMap &row) const {
    const auto body = row.value("body").toString().trimmed();
    return !row.value("subject").toString().trimmed().isEmpty() || !row.value("to_recipients").toList().isEmpty()
        || row.value("attachment_count").toInt() > 0
        || (!body.isEmpty() && body != currentAccount().value("signature_text").toString().trimmed());
}
void MailBackend::dropEmptyNewDraft() {
    // A new message with nothing written is not kept in Drafts. No Undo: there
    // is nothing to bring back. Drafts that came from Gmail stay until the
    // person discards them.
    if (m_draft.value("mode").toString() != "new" || m_activeDraft.isEmpty() || m_draft.value("discarded").toBool()
        || draftReadOnly() || m_draft.value("google").toMap().value("state").toString() == "adopted"
        || draftHasContent() || draftBusy() || accountArchived() || m_unsavedAccountBuffer || !m_connected) return;
    const auto revision = m_draft.value("revision").toLongLong();
    if (revision <= 0) return;
    m_draftTimer.stop(); m_draftDirty = false;
    auto params = draftSelector(); params["request_id"] = mutation()["request_id"]; params["expected_revision"] = revision;
    const auto account = m_account;
    request("draft_discard", account, params, [this, account](auto, bool ok) {
        if (ok && account == m_account) { loadAccounts(); if (m_folder == "drafts") loadDraftRows(false); m_draftChipsTimer.start(); }
    }, true);
}
bool MailBackend::draftAddedFiles() const {
    QSet<QString> starting;
    if (m_draft.value("mode") == "forward")
        for (const auto &value : m_conversation.value("messages").toList()) {
            const auto member = value.toMap();
            if (member.value("id") != m_draft.value("source_message_id")) continue;
            for (const auto &file : member.value("attachments").toList()) starting.insert(file.toMap().value("id").toString());
        }
    for (const auto &id : m_draft.value("attachment_ids").toList()) if (!starting.contains(id.toString())) return true;
    return false;
}
QString MailBackend::authoredDraftText(bool *untouched) const {
    const auto body = m_draft.value("body").toString();
    const auto signature = currentAccount().value("signature_text").toString().trimmed();
    const auto forwarded = QStringLiteral("---------- Forwarded message ----------");
    auto end = signature.isEmpty() ? qsizetype(-1) : body.indexOf(signature);
    if (end < 0) end = body.indexOf(forwarded);
    if (untouched) {
        const auto rest = end < 0 ? QString{} : body.mid(end).trimmed();
        *untouched = rest.isEmpty() || rest == signature || rest.startsWith(forwarded)
            || (!signature.isEmpty() && rest.startsWith(signature + QStringLiteral("\n\n") + forwarded));
    }
    return (end < 0 ? body : body.left(end)).trimmed();
}
void MailBackend::retireDraft(const QString &account, const QJsonObject &selector) {
    if (account != m_account || !m_connected || m_unsavedAccountBuffer) return;
    auto params = selector; params["request_id"] = mutation()["request_id"];
    request("draft_discard", account, params, [this, account](auto, bool ok) {
        if (ok && account == m_account) { loadAccounts(); if (m_folder == "drafts") loadDraftRows(false); m_draftChipsTimer.start(); }
    }, true);
}
void MailBackend::leaveDiscardedNewDraft() {
    if (m_draft.value("mode").toString() != "new" || !m_draft.value("discarded").toBool() || m_draftDirty || draftBusy()) return;
    const auto origin = m_newDraftOrigin; m_newDraftOrigin.clear();
    if (origin.size() == 3 && origin[0] == m_account && !origin[2].isEmpty() && origin[2] != m_activeDraft) { openDraft(origin[2]); return; }
    if (origin.size() == 3 && origin[0] == m_account && !origin[1].isEmpty()) { selectConversationInternal(origin[1], false); return; }
    ++m_generation; m_activeDraft.clear(); m_draft.clear(); m_recipientInputs.clear(); m_draftStatus.clear();
    remember("selectedDraft", ""); remember("replyOpen", false);
    emit conversationChanged(); emit draftChanged(); emit presentationChanged();
}
void MailBackend::leaveNewDraft() {
    if (m_draft.value("mode").toString() != "new" || m_activeDraft.isEmpty()) return;
    if (!draftHasContent()) { dropEmptyNewDraft(); return; }
    if (m_draftDirty) saveDraft();
}
void MailBackend::refreshDraftChips() {
    const auto account = m_account;
    const int generation = ++m_draftChipsGeneration;
    // Chips follow the loaded account list; before it loads there are none.
    if (account.isEmpty() || !m_connected || !m_initialAccountsLoaded || accountArchived()) {
        if (!m_draftChips.isEmpty()) { m_draftChips.clear(); emit draftChipsChanged(); }
        return;
    }
    request("drafts", account, {{"limit", 40}}, [this, account, generation](auto result, bool ok) {
        if (!ok || account != m_account || generation != m_draftChipsGeneration) return;
        QVariantList rows;
        const auto page = result["drafts"].toArray();
        for (const auto &value : page) {
            const auto row = value.toObject().toVariantMap();
            if (!row.value("read_only").toBool() && authoredDraftRow(row)) rows.append(row);
        }
        // The count is every draft in Drafts, as the folder lists them; chips
        // show only the ones with something written.
        const auto total = result["total"].toInteger(page.size());
        const QVariantMap next{{"account", account}, {"rows", rows}, {"total", total}};
        if (next != m_draftChips) { m_draftChips = next; emit draftChipsChanged(); }
    });
}
void MailBackend::loadRecentWith(const QString &email) {
    const auto address = email.trimmed().toLower(), account = m_account;
    if (address.isEmpty() || account.isEmpty() || !validFilterSender(address)) {
        ++m_recentGeneration;
        if (!m_recentWith.isEmpty()) { m_recentWith.clear(); emit recentWithChanged(); }
        return;
    }
    if (m_recentWith.value("email") == address && m_recentWith.value("account") == account) return;
    const int generation = ++m_recentGeneration;
    // A bare address matches mail from, to or copying that person.
    request("conversations", account, {{"query", address}, {"folder", "all"}, {"limit", 4}}, [this, account, address, generation](auto result, bool ok) {
        if (account != m_account || generation != m_recentGeneration) return;
        QVariantList rows;
        if (ok) for (const auto &value : result["conversations"].toArray()) rows.append(value.toObject().toVariantMap());
        m_recentWith = {{"account", account}, {"email", address}, {"rows", rows}, {"total", ok ? result["filtered_conversation_count"].toInt() : 0}};
        emit recentWithChanged();
    });
}
