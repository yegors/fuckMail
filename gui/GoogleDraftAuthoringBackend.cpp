#include "MailBackend.h"
#include <QJsonArray>

namespace {
QString ownerKey(const QString &account, const QString &draft) { return account + '\n' + draft; }
QString reasonText(const QString &code) {
    if (code.contains("from") || code.contains("identity")) return "The original From cannot be preserved by this editor.";
    if (code.contains("html") || code.contains("cid") || code.contains("inline")) return "The source has HTML or inline presentation this editor cannot preserve.";
    if (code.contains("routing") || code.contains("recipient") || code.contains("reply") || code.contains("sender") || code.contains("resent")) return "The original routing or reply headers cannot be preserved.";
    if (code.contains("pending") || code.contains("unknown") || code.contains("owner")) return "Related Google work must be resolved first.";
    if (code.contains("attachment") || code.contains("file")) return "Every original file must be available and supported.";
    if (code.contains("missing") || code.contains("unavailable")) return "The exact original source is unavailable.";
    if (code.contains("scope") || code.contains("capability")) return "This Google account needs the required access.";
    return "The complete source does not support this action.";
}
QString reasons(const QJsonObject &assessment, const QString &action) {
    QStringList text;
    for (const auto &value : assessment[action].toObject()["reasons"].toArray()) {
        const auto next = reasonText(value.toString());
        if (!text.contains(next)) text.append(next);
    }
    return text.join(' ');
}
bool validDeletion(const QJsonObject &status, const QString &account, const QString &draft) {
    static const QStringList states{"prepared", "in_flight", "deleted", "rejected", "acceptance_unknown", "abandoned"};
    return status["account"] == account && (status["draft_id"] == draft || status["related_draft_ids"].toArray().contains(draft))
        && !status["request_id"].toString().isEmpty() && status["revision"].toInteger() > 0
        && status["source_revision"].toInteger() > 0 && status["current_google_revision"].toInteger(-1) >= 0
        && status["operation_is_current"].isBool() && states.contains(status["state"].toString());
}
}

void MailBackend::resetDraftAuthoringContext() {
    const auto key = draftSourcesKey();
    if (key != m_draftAuthoringKey) {
        m_draftAuthoringKey = key; ++m_draftAuthoringGeneration;
        m_draftAuthoring = {}; m_draftSourceConfirmation = {}; m_draftAuthoringFeedback.clear();
        m_draftAuthoringLoading = false;
    }
    const auto id = m_draft.value("draft_id").toString();
    if (m_draftDeletionAccount != m_account || m_draftDeletionDraft != id) {
        m_draftDeletionAccount = m_account; m_draftDeletionDraft = id; ++m_draftDeletionGeneration;
        m_draftDeletionSelected = {}; m_draftDeleteConfirmation = {}; m_draftDeletionFeedback.clear();
        m_draftDeletionLoading = false; m_draftDeletionActive = false;
    }
}

QVariantMap MailBackend::draftAuthoring() const {
    const auto id = m_draft.value("draft_id").toString();
    const auto key = draftSourcesKey();
    const auto ui = m_draftSourceActions.value(ownerKey(m_account, id));
    const auto google = m_draft.value("google").toMap();
    auto origin = google.value("adoption").toMap();
    if (origin.isEmpty()) origin = google.value("derivation").toMap();
    QVariantMap result{{"visible", !key.isEmpty() && draftReadOnly()}, {"loading", m_draftAuthoringLoading},
        {"busy", ui.active}, {"retry", ui.uncertain && ui.action != "delete"},
        {"result_draft_id", ui.resultDraftId}, {"original_draft_id", origin.value("source_draft_id")},
        {"status", ui.feedback.isEmpty() ? m_draftAuthoringFeedback : ui.feedback}};
    const bool current = !key.isEmpty() && key == m_draftAuthoringKey && !m_draftAuthoring.isEmpty();
    const auto deletion = m_draft.value("google_delete").toMap().value("state").toString();
    const bool deletionPending = deletion == "prepared" || deletion == "in_flight" || deletion == "acceptance_unknown";
    const bool ready = current && m_connected && !m_shutdown && draftReadOnly() && !m_draftDirty
        && !draftBusy() && !m_draftAuthoringLoading && !ui.active && !ui.uncertain
        && !m_draftDeletionLoading && !m_draftDeletionActive && !deletionPending;
    const bool owned = !m_syntheticProfile && googleSyncState(m_account).value("owned").toBool();
    for (const auto &action : {QString("copy"), QString("adopt"), QString("delete")}) {
        result[action + "_allowed"] = ready && m_draftAuthoring[action].toObject()["allowed"].toBool()
            && (action == "copy" || (owned && accountCapability(m_account, "mail_delivery")));
        auto explanation = current ? reasons(m_draftAuthoring, action) : QString{};
        if (explanation.isEmpty() && action != "copy" && !owned) explanation = "Start this account's Google sync in Settings first.";
        result[action + "_reason"] = explanation;
    }
    result["assessment"] = current ? m_draftAuthoring.toVariantMap() : QVariantMap{};
    return result;
}

void MailBackend::loadDraftAuthoring() {
    if (!draftReadOnly() || draftSourcesKey().isEmpty() || !m_connected || m_shutdown) return;
    resetDraftAuthoringContext();
    if (m_draftAuthoringLoading) return;
    const auto key = m_draftAuthoringKey, account = m_account, id = m_draft.value("draft_id").toString();
    const auto source = m_draft.value("remote_source").toMap();
    const auto sourceRevision = source.value("source_revision").toLongLong();
    const auto generation = ++m_draftAuthoringGeneration;
    m_draftAuthoringLoading = true; m_draftAuthoringFeedback.clear(); m_draftSourceConfirmation = {};
    emit draftAuthoringChanged();
    googleOperation("google_draft_authoring", account, {{"draft_id", id}, {"source_revision", sourceRevision}},
        [this, key, account, id, source, sourceRevision, generation](auto result, bool ok) {
            if (key != draftSourcesKey() || key != m_draftAuthoringKey || generation != m_draftAuthoringGeneration) return;
            m_draftAuthoringLoading = false;
            if (ok && result["account"] == account && result["draft_id"] == id
                && result["source_revision"].toInteger() == sourceRevision
                && result["source_sha256"].toString() == source.value("sha256").toString()
                && result["provider_draft_id"].toString() == source.value("provider_draft_id").toString()
                && result["primary_from"].isObject() && result["copy"].isObject() && result["adopt"].isObject() && result["delete"].isObject())
                m_draftAuthoring = result;
            else {
                m_draftAuthoring = {};
                m_draftAuthoringFeedback = ok ? "The source changed while its authoring options were checked."
                    : result["message"].toString("The complete source could not be checked.");
            }
            emit draftAuthoringChanged();
        });
}

QVariantMap MailBackend::draftSourceActionContext(const QString &action) {
    m_draftSourceConfirmation = {};
    if ((action != "copy" && action != "adopt" && action != "delete")
        || !draftAuthoring().value(action + "_allowed").toBool()) return {};
    m_draftSourceConfirmation = {{"account", m_account}, {"draft_id", m_draft.value("draft_id").toString()},
        {"source_revision", m_draftAuthoring["source_revision"]}, {"source_sha256", m_draftAuthoring["source_sha256"]},
        {"provider_draft_id", m_draftAuthoring["provider_draft_id"]}, {"primary_from", m_draftAuthoring["primary_from"]},
        {"subject", m_draft.value("subject").toString()}, {"original_from", m_draft.value("from").toString()},
        {"action", action}, {"request_id", mutation()["request_id"]}, {"generation", m_generation},
        {"assessment_generation", qint64(m_draftAuthoringGeneration)}};
    return m_draftSourceConfirmation.toVariantMap();
}

void MailBackend::confirmDraftSourceAction(const QVariantMap &context) {
    const auto captured = QJsonObject::fromVariantMap(context);
    const bool exact = !m_draftSourceConfirmation.isEmpty() && captured == m_draftSourceConfirmation;
    m_draftSourceConfirmation = {};
    const auto action = captured["action"].toString();
    if (!exact || captured["account"] != m_account || captured["draft_id"].toString() != m_draft.value("draft_id").toString()
        || captured["generation"].toInt(-1) != m_generation
        || captured["assessment_generation"].toInteger(-1) != qint64(m_draftAuthoringGeneration)
        || captured["source_revision"] != m_draftAuthoring["source_revision"]
        || captured["source_sha256"] != m_draftAuthoring["source_sha256"]
        || !draftAuthoring().value(action + "_allowed").toBool()) {
        m_draftAuthoringFeedback = "The account, source or available actions changed. Review the current source before confirming.";
        emit draftAuthoringChanged(); return;
    }
    const auto key = ownerKey(m_account, m_draft.value("draft_id").toString());
    if (!m_draftSourceActions.contains(key) && m_draftSourceActions.size() >= 64) {
        for (auto it = m_draftSourceActions.begin(); it != m_draftSourceActions.end() && m_draftSourceActions.size() >= 64;)
            if (!it->active && !it->uncertain) it = m_draftSourceActions.erase(it); else ++it;
        if (m_draftSourceActions.size() >= 64) { m_draftAuthoringFeedback = "Resolve earlier source actions before starting another."; emit draftAuthoringChanged(); return; }
    }
    SourceActionUi ui; ui.action = action;
    ui.request = {{"draft_id", captured["draft_id"]}, {"source_revision", captured["source_revision"]}, {"request_id", captured["request_id"]}};
    ui.source = QJsonObject::fromVariantMap(m_draft.value("remote_source").toMap());
    if (action == "copy") {
        ui.request["from_policy"] = "primary"; ui.request["content_policy"] = "plain_text";
        ui.request["inline_policy"] = "attachments"; ui.request["thread_policy"] = "omit";
    }
    m_draftSourceActions[key] = ui;
    startDraftSourceAction(key);
}

void MailBackend::startDraftSourceAction(const QString &key) {
    if (!m_connected || m_shutdown || !m_draftSourceActions.contains(key) || m_draftSourceActions[key].active) return;
    const auto account = m_account, id = m_draft.value("draft_id").toString();
    if (key != ownerKey(account, id)) return;
    auto &ui = m_draftSourceActions[key];
    const auto params = ui.request; const auto source = ui.source; const auto action = ui.action; const bool priorUncertain = ui.uncertain;
    const auto sourceKey = draftSourcesKey(); const auto navigation = m_generation;
    ui.active = true; ui.feedback = action == "copy" ? "Creating the separate plain-text draft…"
        : action == "adopt" ? "Checking and opening the Google draft…" : "Checking and deleting the exact Google draft…";
    emit draftAuthoringChanged(); emit draftDeletionChanged();
    googleOperation("google_draft_" + action, account, params,
        [this, key, account, id, params, source, action, priorUncertain, sourceKey, navigation](auto result, bool ok) {
            if (!m_draftSourceActions.contains(key) || m_draftSourceActions[key].request != params) return;
            auto &saved = m_draftSourceActions[key]; saved.active = false;
            const auto target = result["google"].toObject()["effective_target"].toObject();
            const bool existingOwner = action == "adopt" && !source["linked_local_draft_id"].toString().isEmpty()
                && result["draft_id"] == source["linked_local_draft_id"]
                && target["provider_draft_id"] == source["provider_draft_id"] && target["message_id"] == source["message_id"]
                && target["sha256"] == source["sha256"] && target["size"] == source["size"];
            const auto origin = result["google"].toObject()[action == "copy" ? "derivation" : "adoption"].toObject();
            if (action == "delete") {
                if (ok && validDeletion(result, account, id) && result["request_id"] == params["request_id"]) {
                    saved.uncertain = false; saved.feedback.clear();
                    mergeDraftDeletion(result, account, id);
                } else {
                    const bool finished = result["details"].toObject()["draft_delete_invocation_finished_before_dispatch"].toBool();
                    // This proof covers only the completed invocation. It cannot
                    // establish absence of a prepared or previously uncertain receipt.
                    saved.uncertain = true;
                    saved.freshDeleteFinished = finished && !priorUncertain;
                    saved.feedback = finished && !priorUncertain
                        ? "This invocation stopped before deletion. Check its exact saved request for any prepared receipt."
                        : "Deletion result not confirmed. Check the exact saved request; no deletion will be retried automatically.";
                    if (account == m_account && id == m_draft.value("draft_id").toString()) loadDraftDeletion(params["request_id"].toString());
                }
            } else if (ok && result["account"] == account && !result["draft_id"].toString().isEmpty()
                && !result["read_only"].toBool() && result["origin"] != "google_snapshot"
                && ((origin["source_draft_id"] == id && origin["source_revision"] == params["source_revision"]) || existingOwner)) {
                saved.uncertain = false; saved.resultDraftId = result["draft_id"].toString();
                saved.feedback = action == "copy" ? "The separate plain-text copy is saved." : "The editable Google draft is available. Adoption alone does not upload.";
                // Receipts can contain an older authored snapshot. Reopen by ID
                // through the normal revision-fenced read, never install it here.
                const auto resultId = saved.resultDraftId;
                if (account == m_account && id == m_draft.value("draft_id").toString()
                    && navigation == m_generation && sourceKey == draftSourcesKey() && !m_draftDirty && !draftBusy() && !m_shutdown)
                    openDraft(resultId);
            } else {
                const bool finished = !ok && result["details"].toObject()["draft_authoring_invocation_finished_without_commit"].toBool();
                saved.uncertain = priorUncertain || !finished;
                saved.feedback = result["message"].toString("The result was not confirmed.")
                    + (saved.uncertain ? " Retry checks this exact action identity before creating or adopting anything."
                        : " Review the source and check its actions again.");
                if (!saved.uncertain && sourceKey == draftSourcesKey()) m_draftAuthoring = {};
            }
            if (account == m_account) {
                emit draftAuthoringChanged(); emit draftDeletionChanged();
                if (m_folder == "drafts") loadDraftRows(false, true);
            }
        });
}

void MailBackend::retryDraftSourceAction() {
    const auto key = ownerKey(m_account, m_draft.value("draft_id").toString());
    const auto ui = m_draftSourceActions.value(key);
    if (!ui.uncertain || ui.active || ui.action == "delete" || !draftReadOnly() || draftBusy() || m_draftDirty) return;
    if (ui.action == "adopt" && !googleSyncState(m_account).value("owned").toBool()) return;
    startDraftSourceAction(key);
}
void MailBackend::openDraftSourceActionResult() {
    const auto id = m_draftSourceActions.value(ownerKey(m_account, m_draft.value("draft_id").toString())).resultDraftId;
    if (!id.isEmpty()) openDraft(id);
}

QVariantMap MailBackend::draftDeletion() const {
    const auto id = m_draft.value("draft_id").toString();
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google_delete").toMap());
    const auto selected = validDeletion(m_draftDeletionSelected, m_account, id) ? m_draftDeletionSelected : current;
    const auto ui = m_draftSourceActions.value(ownerKey(m_account, id));
    const bool pendingUi = ui.action == "delete" && !ui.request.isEmpty() && (ui.active || ui.uncertain);
    const bool visible = validDeletion(selected, m_account, id) || pendingUi || !m_draftDeletionFeedback.isEmpty();
    const auto state = selected["state"].toString();
    QString text;
    if (state == "deleted") text = "Google confirmed this draft deletion. The original saved sources remain available.";
    else if (state == "prepared") text = "Deletion is prepared and not dispatched. Abandon leaves the Google draft unchanged.";
    else if (state == "in_flight") text = "Deletion is in progress. Its result is not confirmed.";
    else if (state == "rejected") text = "Google draft deletion was refused. No success is established.";
    else if (state == "abandoned") text = "This prepared deletion was abandoned before dispatch.";
    else if (state == "acceptance_unknown") text = "Deletion acceptance is unknown. Check reads Google without retrying deletion.";
    if (ui.uncertain && ui.action == "delete") text += " " + ui.feedback;
    const auto evidence = selected["read_evidence"].toObject();
    if (!evidence.isEmpty() && evidence["draft_exists"].isBool()) text += evidence["draft_exists"].toBool()
        ? " The checked draft is present on Google."
        : " The checked draft is absent. This does not establish sending or confirm this deletion request.";
    if (!m_draftDeletionFeedback.isEmpty()) text += " " + m_draftDeletionFeedback;
    const bool busy = m_draftDeletionActive || m_draftDeletionLoading || (ui.action == "delete" && ui.active);
    const auto requestId = selected["request_id"].toString().isEmpty() ? ui.request["request_id"].toString() : selected["request_id"].toString();
    const bool connected = m_connected && !m_shutdown && !requestId.isEmpty();
    return {{"visible", visible}, {"status", text.trimmed()}, {"busy", busy}, {"receipt", selected.toVariantMap()},
        {"request_id", requestId}, {"can_status", connected && !busy},
        {"can_check", connected && !busy && googleSyncState(m_account).value("owned").toBool()
            && accountCapability(m_account, "mail_read") && state != "in_flight"},
        {"can_abandon", connected && !busy && state == "prepared"},
        {"warning", ui.uncertain || state == "acceptance_unknown" || state == "rejected"}};
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
    const auto priorSelected = m_draftDeletionSelected;
    bool feedbackChanged = false;
    if (status["operation_is_current"].toBool()
        && (!validDeletion(current, account, draftId)
            || (status["current_google_revision"].toInteger() >= current["current_google_revision"].toInteger()
                && (status["request_id"] != current["request_id"] || status["revision"].toInteger() >= current["revision"].toInteger()))))
        m_draft["google_delete"] = status.toVariantMap();
    if (m_draftDeletionSelected.isEmpty() || m_draftDeletionSelected["request_id"] == status["request_id"]) {
        if (m_draftDeletionSelected["request_id"] != status["request_id"]
            || status["revision"].toInteger() >= m_draftDeletionSelected["revision"].toInteger()) m_draftDeletionSelected = status;
    }
    const auto key = ownerKey(account, draftId);
    if (m_draftSourceActions.contains(key) && m_draftSourceActions[key].action == "delete"
        && m_draftSourceActions[key].request["request_id"] == status["request_id"]) {
        feedbackChanged = m_draftSourceActions[key].uncertain || !m_draftSourceActions[key].feedback.isEmpty();
        m_draftSourceActions[key].uncertain = false; m_draftSourceActions[key].feedback.clear();
        m_draftSourceActions[key].freshDeleteFinished = false;
    }
    const bool changed = current != QJsonObject::fromVariantMap(m_draft.value("google_delete").toMap())
        || priorSelected != m_draftDeletionSelected || feedbackChanged;
    if (changed) { emit draftDeletionChanged(); emit draftAuthoringChanged(); }
    return changed;
}

void MailBackend::loadDraftDeletion(const QString &requestId) {
    if (!m_connected || m_shutdown || m_draft.value("draft_id").toString().isEmpty()) return;
    const auto account = m_account, id = m_draft.value("draft_id").toString();
    auto selectedId = requestId;
    if (selectedId.isEmpty()) selectedId = draftDeletion().value("request_id").toString();
    if (selectedId.isEmpty() || selectedId.toUtf8().size() > 128 || m_draftDeletionLoading) return;
    const auto generation = ++m_draftDeletionGeneration;
    m_draftDeletionLoading = true; m_draftDeletionFeedback.clear();
    emit draftDeletionChanged();
    request("google_draft_delete_status", account, {{"request_id", selectedId}},
        [this, account, id, selectedId, generation](auto result, bool ok) {
            if (account != m_account || id != m_draft.value("draft_id").toString() || generation != m_draftDeletionGeneration) return;
            m_draftDeletionLoading = false;
            if (ok && validDeletion(result, account, id) && result["request_id"] == selectedId) {
                if (m_draftDeletionSelected["request_id"] != selectedId) m_draftDeletionSelected = {};
                mergeDraftDeletion(result, account, id);
            } else {
                const auto key = ownerKey(account, id);
                if (result["code"] == "not_found" && m_draftSourceActions.contains(key)
                    && m_draftSourceActions[key].request["request_id"] == selectedId
                    && m_draftSourceActions[key].freshDeleteFinished) {
                    auto &ui = m_draftSourceActions[key];
                    ui.uncertain = false; ui.freshDeleteFinished = false;
                    ui.feedback = "The completed invocation did not delete anything and has no saved receipt. Review the current source before confirming a new action.";
                    m_draftAuthoring = {};
                } else m_draftDeletionFeedback = result["code"] == "not_found"
                    ? "No saved receipt was found. This is not proof that an earlier uncertain deletion cannot complete."
                    : result["message"].toString("The deletion receipt could not be read.");
            }
            emit draftDeletionChanged(); emit draftAuthoringChanged();
        });
}

void MailBackend::checkDraftDeletion() {
    if (!draftDeletion().value("can_check").toBool()) return;
    const auto account = m_account, id = m_draft.value("draft_id").toString(), requestId = draftDeletion().value("request_id").toString();
    const auto generation = ++m_draftDeletionGeneration;
    m_draftDeletionActive = true; m_draftDeletionFeedback = "Checking Google without deleting…"; emit draftDeletionChanged();
    googleOperation("google_draft_delete_reconcile", account, {{"request_id", requestId}},
        [this, account, id, requestId, generation](auto result, bool ok) {
            if (account != m_account || id != m_draft.value("draft_id").toString() || generation != m_draftDeletionGeneration) return;
            m_draftDeletionActive = false;
            if (ok && result["request_id"] == requestId && validDeletion(result, account, id)) {
                mergeDraftDeletion(result, account, id); m_draftDeletionFeedback.clear();
            }
            else m_draftDeletionFeedback = result["message"].toString("Google could not be checked. The saved deletion outcome is unchanged.");
            emit draftDeletionChanged();
        });
}

QVariantMap MailBackend::draftDeletionAbandonContext() {
    m_draftDeleteConfirmation = {};
    if (!draftDeletion().value("can_abandon").toBool()) return {};
    const auto receipt = draftDeletion().value("receipt").toMap();
    m_draftDeleteConfirmation = {{"account", m_account}, {"draft_id", m_draft.value("draft_id").toString()},
        {"request_id", receipt.value("request_id").toString()}, {"expected_revision", receipt.value("revision").toLongLong()},
        {"generation", m_generation}};
    return m_draftDeleteConfirmation.toVariantMap();
}
void MailBackend::abandonDraftDeletion(const QVariantMap &context) {
    const auto captured = QJsonObject::fromVariantMap(context);
    const bool same = !m_draftDeleteConfirmation.isEmpty() && captured == m_draftDeleteConfirmation;
    m_draftDeleteConfirmation = {};
    const auto receipt = draftDeletion().value("receipt").toMap();
    if (!same || !draftDeletion().value("can_abandon").toBool() || captured["account"] != m_account
        || captured["draft_id"].toString() != m_draft.value("draft_id").toString() || captured["generation"].toInt(-1) != m_generation
        || captured["request_id"].toString() != receipt.value("request_id").toString()
        || captured["expected_revision"].toInteger() != receipt.value("revision").toLongLong()) return;
    const auto account = m_account, id = m_draft.value("draft_id").toString();
    const auto generation = ++m_draftDeletionGeneration;
    m_draftDeletionActive = true; emit draftDeletionChanged();
    request("google_draft_delete_abandon", account, {{"request_id", captured["request_id"]}, {"expected_revision", captured["expected_revision"]}},
        [this, account, id, generation](auto result, bool ok) {
            if (account != m_account || id != m_draft.value("draft_id").toString() || generation != m_draftDeletionGeneration) return;
            m_draftDeletionActive = false;
            if (ok && validDeletion(result, account, id)) { mergeDraftDeletion(result, account, id); m_draftDeletionFeedback.clear(); }
            else m_draftDeletionFeedback = result["message"].toString("The prepared deletion could not be abandoned.");
            emit draftDeletionChanged();
        });
}
