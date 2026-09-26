// MailBackend's Gmail copy of the open draft: merging its sync status and any
// deletion receipt into the draft, the plain hint line the composer shows about
// it, and the explicit check and replace operations.
#include "MailBackend.h"
#include "GoogleOperation.h"
#include <QJsonArray>

namespace {
// A Gmail-copy status is usable only for this exact draft, with non-negative
// revisions and a known state, and not flagged as a historical receipt.
bool validStatus(const QJsonObject &status, const QString &account, const QString &draft) {
    static const QStringList states{"local", "adopted", "prepared", "in_flight", "acceptance_unknown", "acknowledged", "synced", "conflict", "failed"};
    return !account.isEmpty() && !draft.isEmpty() && status["account"].toString() == account
        && status["draft_id"].toString() == draft && status["revision"].isDouble()
        && status["revision"].toInteger(-1) >= 0 && status["local_revision"].isDouble()
        && status["local_revision"].toInteger(-1) >= 0 && states.contains(status["state"].toString())
        && status["operation_is_current"] != QJsonValue(false);
}
// Statuses order by their own revision, then by the local draft revision they
// describe. Replies from separate connections can arrive out of order.
bool olderStatus(const QJsonObject &candidate, const QJsonObject &current) {
    return candidate["revision"].toInteger() < current["revision"].toInteger()
        || (candidate["revision"] == current["revision"]
            && candidate["local_revision"].toInteger() < current["local_revision"].toInteger());
}
// A Gmail deletion receipt is usable only for this draft (or one it names as
// related), with its request, revisions and a known state all present.
bool validDeletion(const QJsonObject &status, const QString &account, const QString &draft) {
    static const QStringList states{"prepared", "in_flight", "deleted", "rejected", "acceptance_unknown", "abandoned"};
    return status["account"] == account && (status["draft_id"] == draft || status["related_draft_ids"].toArray().contains(draft))
        && !status["request_id"].toString().isEmpty() && status["revision"].toInteger() > 0
        && status["source_revision"].toInteger() > 0 && status["current_google_revision"].toInteger(-1) >= 0
        && status["operation_is_current"].isBool() && states.contains(status["state"].toString());
}
QString preparationReason(const QString &code) {
    if (code == "limit_exceeded") return "It's larger than Gmail's 35 MB limit.";
    if (code == "invalid_identity") return "The From address is invalid.";
    if (code == "identity_mismatch") return "It's from an address other than this account's.";
    return "Some addresses, formatting or attachments aren't valid.";
}
}

QVariantMap MailBackend::mergedDraftProjection(const QJsonObject &result) const {
    auto next = result.toVariantMap();
    mergeDraftDeletionProjection(next, result);
    if (result["read_only"].toBool() || result["origin"].toString() == "google_snapshot") {
        next["google"] = QVariant{}; next["google_send"] = QVariant{};
        return next;
    }
    const auto account = result["account"].toString(), id = result["draft_id"].toString();
    const auto incoming = result["google"].toObject();
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google").toMap());
    // Keep the status already held when the incoming one is missing, invalid
    // or older, so a late read cannot roll the Gmail-copy state back.
    const bool sameDraft = m_draft.value("account").toString() == account && m_draft.value("draft_id").toString() == id;
    if (sameDraft && validStatus(current, account, id)
        && (!validStatus(incoming, account, id) || olderStatus(incoming, current))) next["google"] = current.toVariantMap();
    else if (result.contains("google") && !validStatus(incoming, account, id)) next["google"] = QVariant{};
    mergeGoogleSendProjection(next, result);
    return next;
}

bool MailBackend::mergeGoogleDraftStatus(const QJsonObject &status, const QString &account, const QString &draftId) {
    if (draftReadOnly()) return false;
    if (account != m_account || draftId != m_draft.value("draft_id").toString()
        || !validStatus(status, account, draftId)) return false;
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google").toMap());
    if (validStatus(current, account, draftId) && olderStatus(status, current)) return false;
    if (status == current) return false;
    m_draft["google"] = status.toVariantMap();
    if (status["revision"].toInteger() > current["revision"].toInteger(-1)
        && m_googleDraftFeedbackAccount == account && m_googleDraftFeedbackId == draftId) m_googleDraftFeedback.clear();
    return true;
}

void MailBackend::mergeDraftDeletionProjection(QVariantMap &next, const QJsonObject &result) const {
    const auto account = result["account"].toString(), id = result["draft_id"].toString();
    const auto incoming = result["google_delete"].toObject();
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google_delete").toMap());
    const bool same = m_draft.value("account").toString() == account && m_draft.value("draft_id").toString() == id;
    if (same && validDeletion(current, account, id)
        && (!validDeletion(incoming, account, id) || !incoming["operation_is_current"].toBool()
            || incoming["current_google_revision"].toInteger() < current["current_google_revision"].toInteger()
            || (incoming["request_id"] == current["request_id"] && incoming["revision"].toInteger() < current["revision"].toInteger())))
        next["google_delete"] = current.toVariantMap();
    else if (!validDeletion(incoming, account, id) || !incoming["operation_is_current"].toBool()) next["google_delete"] = QVariant{};
}

bool MailBackend::mergeDraftDeletion(const QJsonObject &status, const QString &account, const QString &draftId) {
    if (account != m_account || draftId != m_draft.value("draft_id").toString() || !validDeletion(status, account, draftId)) return false;
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google_delete").toMap());
    if (status["operation_is_current"].toBool()
        && (!validDeletion(current, account, draftId)
            || (status["current_google_revision"].toInteger() >= current["current_google_revision"].toInteger()
                && (status["request_id"] != current["request_id"] || status["revision"].toInteger() >= current["revision"].toInteger()))))
        m_draft["google_delete"] = status.toVariantMap();
    return current != QJsonObject::fromVariantMap(m_draft.value("google_delete").toMap());
}

bool MailBackend::googleDraftRecoveryReady() const {
    return !draftReadOnly() && !m_syntheticProfile && m_connected && !m_shutdown && accountCapability(m_account, "mail_delivery")
        && !m_draft.value("draft_id").toString().isEmpty() && m_draft.value("revision").toLongLong() > 0
        && !m_draft.value("discarded").toBool() && !googleSendBlocksDraftSync()
        && !m_draft.value("google").toMap().value("deletion").toMap().value("blocks_target").toBool();
}

QVariantMap MailBackend::googleDraft() const {
    if (draftReadOnly()) return {{"visible", false}};
    const auto id = m_draft.value("draft_id").toString();
    const auto status = QJsonObject::fromVariantMap(m_draft.value("google").toMap());
    const bool visible = !m_syntheticProfile && validStatus(status, m_account, id);
    const auto state = status["state"].toString();
    const bool busy = m_googleDraftOperationAccount == m_account && m_googleDraftOperationId == id && !id.isEmpty();
    const auto baseline = status["baseline"].toObject();
    const auto operation = status["operation"].toObject();
    const auto observed = status["conflict"].toObject()["observed"].toObject();
    const bool unknown = state == "acceptance_unknown" || operation["state"] == "acceptance_unknown";
    const auto target = operation["provider_draft_id"].toString();
    const bool observedUnknownTarget = !target.isEmpty() && observed["draft_id"].toString() == target;
    const bool knownRemote = !target.isEmpty() || !operation["ack"].toObject()["draft_id"].toString().isEmpty()
        || !baseline["draft_id"].toString().isEmpty() || !status["effective_target"].toObject()["provider_draft_id"].toString().isEmpty();
    const auto preparationFailure = status["preparation_failure"].toObject();
    const bool mismatch = !baseline["observed_sha256"].toString().isEmpty() && !baseline["upload_sha256"].toString().isEmpty()
        && baseline["observed_sha256"] != baseline["upload_sha256"];
    // Drafts are Gmail drafts: syncing, conflicts and Gmail's own edits
    // resolve on their own. Only what the person should know is said, plainly.
    QString text;
    if (state == "failed") text = "Couldn't save this draft to Gmail. It's kept here.";
    else if (unknown && target.isEmpty()) text = "Gmail hasn't confirmed this draft yet. It's kept here.";
    if (!preparationFailure.isEmpty() && !m_draftDirty
        && m_draft.value("revision").toLongLong() <= preparationFailure["draft_revision"].toInteger()) {
        auto reason = preparationReason(preparationFailure["code"].toString());
        reason[0] = reason[0].toLower();
        text = "Couldn't save this draft to Gmail: " + reason;
    }
    if (googleSendBlocksDraftSync()) return {{"visible", false}, {"can_check", false}, {"can_replace", false}};
    if (busy) text += m_googleDraftOperationMethod == "google_draft_reconcile" ? " Checking Google…" : " Replacing the Google copy…";
    if (m_googleDraftFeedbackAccount == m_account && m_googleDraftFeedbackId == id && !m_googleDraftFeedback.isEmpty()) text += " " + m_googleDraftFeedback;
    const bool ready = visible && googleDraftRecoveryReady() && m_googleDraftOperationId.isEmpty();
    const bool requiresCheck = m_googleDraftRequiresCheck.contains(m_account + '\n' + id);
    const bool warning = !text.isEmpty() && (state == "failed" || !preparationFailure.isEmpty());
    const bool feedback = m_googleDraftFeedbackAccount == m_account && m_googleDraftFeedbackId == id && !m_googleDraftFeedback.isEmpty();
    return {{"visible", visible}, {"state", state}, {"status", text}, {"busy", busy}, {"byte_mismatch", mismatch},
        {"warning", warning},
        // The composer shows draft sync only when something needs the person: a
        // problem, its feedback or a check they started. Normal sync stays silent.
        {"attention", visible && !text.isEmpty() && (warning || feedback || busy || unknown)},
        {"can_check", ready && knownRemote && state != "local"},
        {"can_replace", ready && !requiresCheck && state == "conflict" && (!unknown || observedUnknownTarget)
            && !m_draftDirty && !draftBusy() && !attachmentImportPending() && !m_draftConflict}};
}

void MailBackend::reconcileGoogleDraft() {
    if (!googleDraft().value("can_check").toBool()) return;
    startGoogleDraftOperation("google_draft_reconcile", {{"draft_id", m_draft.value("draft_id").toString()}});
}

QVariantMap MailBackend::googleDraftReplacementContext() {
    m_googleDraftReplacement = {};
    if (!googleDraft().value("can_replace").toBool()) return {};
    m_googleDraftReplacement = {{"account", m_account}, {"draft_id", m_draft.value("draft_id").toString()},
        {"expected_revision", m_draft.value("revision").toLongLong()},
        {"expected_google_revision", m_draft.value("google").toMap().value("revision").toLongLong()},
        {"request_id", mutation()["request_id"]}, {"action", "replace_local"}, {"generation", m_generation}};
    return m_googleDraftReplacement.toVariantMap();
}

void MailBackend::replaceGoogleDraft(const QVariantMap &context) {
    const auto captured = QJsonObject::fromVariantMap(context);
    const bool same = !m_googleDraftReplacement.isEmpty() && captured == m_googleDraftReplacement;
    m_googleDraftReplacement = {};
    if (!same || captured["account"].toString() != m_account || captured["draft_id"].toString() != m_draft.value("draft_id").toString()
        || captured["generation"].toInt(-1) != m_generation) return;
    if (!googleDraft().value("can_replace").toBool()
        || captured["expected_revision"].toInteger() != m_draft.value("revision").toLongLong()
        || captured["expected_google_revision"].toInteger() != m_draft.value("google").toMap().value("revision").toLongLong()) {
        m_googleDraftFeedbackAccount = m_account; m_googleDraftFeedbackId = captured["draft_id"].toString();
        m_googleDraftFeedback = "The draft changed while confirmation was open. Review its current saved versions before replacing.";
        emit draftChanged(); return;
    }
    auto params = captured; params.remove("account"); params.remove("generation");
    startGoogleDraftOperation("google_draft_resolve", params);
}

void MailBackend::startGoogleDraftOperation(const QString &method, const QJsonObject &params) {
    if (draftReadOnly()) return;
    const auto account = m_account, id = m_draft.value("draft_id").toString();
    const auto generation = m_generation;
    m_googleDraftOperationAccount = account; m_googleDraftOperationId = id; m_googleDraftOperationMethod = method;
    m_googleDraftFeedbackAccount = account; m_googleDraftFeedbackId = id; m_googleDraftFeedback.clear();
    emit draftChanged();
    googleOperation(method, account, params, [this, account, id, generation, method](auto result, bool ok) {
        m_googleDraftOperationAccount.clear(); m_googleDraftOperationId.clear(); m_googleDraftOperationMethod.clear();
        const auto key = account + '\n' + id;
        // A replace whose outcome was not applied to the editor must be
        // checked before another replace is offered for the same draft.
        if (account != m_account || id != m_draft.value("draft_id").toString() || generation != m_generation) {
            if (method == "google_draft_resolve") m_googleDraftRequiresCheck.insert(key);
            emit draftChanged(); return;
        }
        if (ok && result["operation_is_current"] == QJsonValue(false)
            && result["account"].toString() == account && result["draft_id"].toString() == id) {
            if (method == "google_draft_resolve") m_googleDraftRequiresCheck.insert(key);
            // An exact historical receipt describes its original operation. Its
            // current twin revision does not make that old state current again.
            // Fetch only cached metadata; this must never reload authored fields.
            m_googleDraftFeedback = "An earlier Google operation was returned. Loading the current draft status…";
            const auto loadingFeedback = m_googleDraftFeedback;
            request("draft_get", account, {{"draft_id", id}}, [this, account, id, generation, loadingFeedback](auto current, bool currentOk) {
                if (account != m_account || id != m_draft.value("draft_id").toString() || generation != m_generation) return;
                const auto metadata = current["google"].toObject();
                if (currentOk && current["account"].toString() == account && current["draft_id"].toString() == id
                    && validStatus(metadata, account, id)) {
                    mergeGoogleDraftStatus(metadata, account, id);
                    if (m_googleDraftFeedback == loadingFeedback) m_googleDraftFeedback.clear();
                } else if (m_googleDraftFeedback == loadingFeedback) m_googleDraftFeedback = "The current draft status could not be read. Check Google before replacing.";
                emit draftChanged();
            });
            emit draftChanged(); return;
        }
        if (method == "google_draft_resolve" && (!ok || !validStatus(result, account, id))) m_googleDraftRequiresCheck.insert(key);
        else if (ok && validStatus(result, account, id)
            && !olderStatus(result, QJsonObject::fromVariantMap(m_draft.value("google").toMap()))) m_googleDraftRequiresCheck.remove(key);
        if (ok && !validStatus(result, account, id)) {
            ok = false; result = QJsonObject{{"message", "Google returned a different draft identity. Check Google to read its saved outcome."}};
        }
        if (ok) mergeGoogleDraftStatus(result, account, id);
        else {
            m_googleDraftFeedbackAccount = account; m_googleDraftFeedbackId = id;
            m_googleDraftFeedback = "Result not confirmed. " + result["message"].toString("Check Google for its saved outcome.");
        }
        // Only Google metadata changes here. A response never replaces author
        // fields, advances the local draft revision or repeats a mutation.
        emit draftChanged();
    });
}

void MailBackend::cancelGoogleDraftOperation() {
    if (m_googleDraftOperationAccount != m_account || m_googleDraftOperationId != m_draft.value("draft_id").toString()) return;
    const auto operations = m_googleOperations;
    for (auto operation : operations)
        if (operation->account() == m_googleDraftOperationAccount && operation->method() == m_googleDraftOperationMethod) operation->cancel();
}
