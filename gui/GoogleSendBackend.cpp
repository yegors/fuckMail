// MailBackend's Send through Gmail: saving the draft first, starting the send
// operation, merging its status into the draft, and check, resume, abandon or
// cancel for a send whose result is unconfirmed.
#include "MailBackend.h"
#include "GoogleOperation.h"
#include <QJsonArray>
#include <algorithm>

namespace {
// A send status is usable only for this draft, in a known state, and with its
// four-field request (draft, request id, both expected revisions) consistent
// with the status itself.
bool validSend(const QJsonObject &status, const QString &account, const QString &draft) {
    static const QStringList states{"prepared", "in_flight", "accepted", "rejected", "acceptance_unknown", "abandoned"};
    const auto request = status["request"].toObject();
    return !account.isEmpty() && !draft.isEmpty() && status["account"] == account && status["draft_id"] == draft
        && !status["request_id"].toString().isEmpty() && states.contains(status["state"].toString())
        && status["revision"].toInteger(-1) > 0 && status["current_google_revision"].toInteger(-1) >= 0
        && status["draft_revision"].toInteger(-1) > 0 && status["google_revision"].toInteger(-1) >= 0 && status["operation_is_current"].isBool()
        && request.size() == 4 && request["draft_id"] == draft && request["request_id"] == status["request_id"]
        && request["expected_revision"] == status["draft_revision"] && request["expected_google_revision"] == status["google_revision"];
}
// True when `next` is older than `current`: an older Gmail-copy revision, or an
// older revision of the same request. Stops late replies from rolling back.
bool earlierSend(const QJsonObject &next, const QJsonObject &current) {
    return next["current_google_revision"].toInteger() < current["current_google_revision"].toInteger()
        || (next["request_id"] == current["request_id"] && next["revision"].toInteger() < current["revision"].toInteger());
}
bool pendingSend(const QString &state) { return state == "prepared" || state == "in_flight" || state == "acceptance_unknown"; }
QString sendSummary(const QJsonObject &status) {
    if (status.isEmpty()) return {};
    const auto state = status["state"].toString();
    QString text;
    if (state == "prepared") text += "Not sent yet. Send the saved message, or cancel to keep editing.";
    else if (state == "in_flight") text += "Sending…";
    else if (state == "accepted") text += "Sent.";
    else if (state == "rejected") text += "Not sent. Google refused the message. Your draft is saved.";
    else if (state == "abandoned") text += "Send cancelled. Your draft is saved.";
    else text += "We could not confirm whether this email was sent. Check status before trying again to avoid sending a duplicate.";
    const auto evidence = status["read_evidence"].toObject();
    if (!evidence.isEmpty()) {
        text += QString(" Read-only check found %1 matching Sent message(s)%2. The original outcome remains %3.")
            .arg(evidence["matches"].toArray().size()).arg(evidence["truncated"].toBool() ? " (results truncated)" : "")
            .arg(state == "acceptance_unknown" ? "unknown" : state);
    }
    return text;
}
}

QString MailBackend::googleSendKey() const { return m_account + '\n' + m_draft.value("draft_id").toString(); }

void MailBackend::mergeGoogleSendProjection(QVariantMap &next, const QJsonObject &result) const {
    const auto account = result["account"].toString(), id = result["draft_id"].toString();
    const auto incoming = result["google_send"].toObject();
    auto current = m_googleSendUi.value(account + '\n' + id).current;
    const auto displayed = QJsonObject::fromVariantMap(m_draft.value("google_send").toMap());
    if (validSend(displayed, account, id) && (current.isEmpty() || !earlierSend(displayed, current))) current = displayed;
    if (validSend(current, account, id)
        && (!validSend(incoming, account, id) || !incoming["operation_is_current"].toBool() || earlierSend(incoming, current)))
        next["google_send"] = current.toVariantMap();
    else if (result.contains("google_send") && (!validSend(incoming, account, id) || !incoming["operation_is_current"].toBool()))
        next["google_send"] = QVariant{};
}

bool MailBackend::mergeGoogleSendStatus(const QJsonObject &status, const QString &account, const QString &draftId) {
    if (draftReadOnly()) return false;
    if (account != m_account || draftId != m_draft.value("draft_id").toString() || !validSend(status, account, draftId)
        || !status["operation_is_current"].toBool()) return false;
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google_send").toMap());
    if (validSend(current, account, draftId) && earlierSend(status, current)) return false;
    bool changed = status != current;
    m_draft["google_send"] = status.toVariantMap();
    if (m_googleSendUi.contains(googleSendKey())) {
        auto &ui = m_googleSendUi[googleSendKey()]; ui.current = status;
        if ((ui.selected["request_id"] == status["request_id"] || ui.request["request_id"] == status["request_id"])
            && (ui.selected.isEmpty() || !earlierSend(status, ui.selected))) {
            changed = changed || ui.selected != status || ui.uncertain || ui.notFound || !ui.feedback.isEmpty();
            ui.selected = status; ui.request = status["request"].toObject();
            ui.uncertain = false; ui.notFound = false; ui.feedback.clear();
        }
    }
    return changed;
}

bool MailBackend::googleSendBlocksDraftSync() const {
    const auto current = m_draft.value("google_send").toMap();
    const auto ui = m_googleSendUi.value(googleSendKey());
    return sendQueued() || !m_googleSendPending.isEmpty() || ui.active || ui.uncertain
        || pendingSend(current.value("state").toString()) || current.value("last_accepted_draft_revision").toLongLong() > 0;
}

QVariantMap MailBackend::googleSend() const {
    if (draftReadOnly()) return {{"visible", false}, {"can_send", false}};
    const auto id = m_draft.value("draft_id").toString();
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google_send").toMap());
    const auto ui = m_googleSendUi.value(googleSendKey());
    const bool otherRequest = !ui.request.isEmpty() && ui.request["request_id"] != current["request_id"];
    const auto selected = ui.selected.isEmpty() ? (otherRequest ? QJsonObject{} : current) : ui.selected;
    const auto state = selected["state"].toString();
    const bool saving = !m_googleSendPending.isEmpty();
    bool hasRecipients = !m_recipientInputs.isEmpty();
    for (const auto field : {"to_recipients", "cc_recipients", "bcc_recipients"})
        hasRecipients = hasRecipients || !m_draft.value(field).toList().isEmpty();
    const bool authorized = !m_syntheticProfile && m_connected && !m_shutdown && accountCapability(m_account, "send");
    const auto sync = googleSyncState(m_account);
    const bool ready = authorized && sync.value("active").toBool();
    // Autosave and the background Gmail copy come and go while the person
    // types. They leave Send enabled; pressing it waits for them (sendNow).
    const bool otherBusy = m_capturing || m_discarding || m_undoingDiscards || draftDiscardActive() || m_resolvingDraft || !m_draftTransferActive.isEmpty();
    const bool cleanTarget = !draftLocked() && !m_draft.isEmpty() && !m_draft.value("discarded").toBool() && !m_draftConflict
        && !otherBusy && !attachmentImportPending() && m_googleDraftOperationId.isEmpty();
    const auto accepted = current["last_accepted_draft_revision"].toInteger();
    const bool newer = m_draftDirty || !m_recipientInputs.isEmpty() || m_draft.value("revision").toLongLong() > accepted;
    const auto google = m_draft.value("google").toMap();
    const auto googleState = google.value("state").toString();
    const bool draftUploading = googleState == "in_flight" || googleState == "prepared" || googleState == "acknowledged";
    const bool draftBlocked = googleState == "acceptance_unknown" || googleState == "conflict";
    const bool deletionBlocked = google.value("deletion").toMap().value("blocks_target").toBool();
    const bool selectedCurrent = validSend(selected, m_account, id) && selected["operation_is_current"].toBool()
        && (current.isEmpty() || (selected["request_id"] == current["request_id"] && !earlierSend(selected, current)));
    const bool sentCurrent = accepted > 0 && !newer && !pendingSend(current["state"].toString());
    QString text = sendSummary(selected);
    if (selected.isEmpty() && authorized && !ready && !m_draft.isEmpty())
        text = sync.value("changing").toBool() ? "Connecting to Google… Your draft is saved." : "Sync is paused. Resume sync to send this draft.";
    if (ui.uncertain) text += " The connection was interrupted. Check status before trying again.";
    if (ui.notFound) text += " No send record was found. Resume checks the original attempt before sending; do not create a duplicate.";
    if (!selected.isEmpty() && !current.isEmpty() && selected["request_id"] != current["request_id"])
        text += " This is an earlier request. Current submission: " + sendSummary(current);
    if (accepted > 0 && newer) text += " Your earlier message was sent. Sending these newer edits creates another email.";
    if (sendQueued()) text = m_sendUndoMs > 0 ? "Sending shortly. Undo keeps this saved draft." : "Sending…";
    else if (saving) text = "Sending…";
    else if (ui.active) {
        if (ui.method == "google_send") text = "Sending…";
        else text += " Checking send status…";
    }
    if (!ui.feedback.isEmpty()) text += " " + ui.feedback;
    const bool canSend = ready && hasRecipients && cleanTarget && !saving && !ui.active && !ui.uncertain && !deletionBlocked && (accepted > 0 || !draftBlocked)
        && !pendingSend(current["state"].toString()) && (accepted == 0 || newer);
    return {{"visible", !m_syntheticProfile && (!text.isEmpty() || !current.isEmpty())}, {"status", text.trimmed()},
        {"sent_current", sentCurrent}, {"can_resume_sync", authorized && !ready && !sync.value("changing").toBool()},
        {"warning", ui.uncertain || state == "acceptance_unknown" || state == "rejected" || (accepted > 0 && newer)},
        {"busy", ui.active || saving || sendQueued()}, {"request_id", selected["request_id"].toString(ui.request["request_id"].toString())},
        {"can_cancel", sendQueued() || saving || (ui.active && ui.method != "google_send_status")},
        {"can_select_current", !ui.active && !saving && !ui.uncertain && validSend(current, m_account, id) && !selected.isEmpty() && current["request_id"] != selected["request_id"]},
        {"check_label", !ui.uncertain && state == "acceptance_unknown" && ready ? "Check Google" : "Check status"},
        {"state", state}, {"can_send", canSend},
        {"send_now", canSend && !m_savingDraft && (accepted > 0 || !draftUploading)},
        {"can_check", !sentCurrent && m_connected && !m_shutdown && !ui.active && !saving && (!selected.isEmpty() || !ui.request.isEmpty())},
        {"resume_label", ui.uncertain ? "Resume original request" : "Send prepared version"},
        {"can_resume", ready && !ui.active && !saving && ((!ui.uncertain && selectedCurrent && state == "prepared")
            || (ui.uncertain && ui.notFound && ui.selected.isEmpty() && !ui.request.isEmpty()))},
        {"can_abandon", !accountArchived() && !m_unsavedAccountBuffer && m_connected && !m_shutdown && !ui.active && !saving && !ui.uncertain && selectedCurrent && state == "prepared"}};
}

QJsonObject MailBackend::googleSendAuthorSnapshot() const {
    QJsonObject result;
    for (const auto key : {"body", "body_html", "signature_text", "attachment_ids", "subject", "to_recipients", "cc_recipients",
             "bcc_recipients", "from", "from_name", "source_message_id", "source_hash"})
        result[key] = QJsonValue::fromVariant(m_draft.value(key));
    result["recipient_inputs"] = QJsonObject::fromVariantMap(recipientInputs());
    return result;
}

void MailBackend::cancelPendingGoogleSend(const QString &reason) {
    if (m_googleSendPending.isEmpty()) return;
    m_googleSendPending = {};
    if (!reason.isEmpty()) m_googleSendUi[googleSendKey()].feedback = reason;
    emit draftChanged();
}

void MailBackend::sendGoogleDraft() {
    if (!googleSend().value("can_send").toBool()) return;
    // No continuation exists until recipient input has been committed. Failed
    // validation must never leave a Send armed for a later autosave.
    if (!commitRecipientInputs()) return;
    if (!googleSend().value("can_send").toBool()) return;
    // The durable journal owns history. Bound presentation-only saved receipts;
    // unresolved request identities are never evicted to make room for a send.
    if (m_googleSendUi.size() >= 64 && !m_googleSendUi.contains(googleSendKey())) {
        for (auto it = m_googleSendUi.begin(); it != m_googleSendUi.end() && m_googleSendUi.size() >= 64;) {
            if (!it->active && !it->uncertain) it = m_googleSendUi.erase(it); else ++it;
        }
        if (m_googleSendUi.size() >= 64) { fail("Check outstanding send requests before starting another."); return; }
    }
    m_googleSendUi[googleSendKey()].feedback.clear();
    // Send what is on screen now: save first, and finishGoogleSendSave() only
    // proceeds if the saved draft still matches this snapshot.
    m_googleSendPending = {{"account", m_account}, {"context", draftContext()}, {"generation", m_generation},
        {"authored", googleSendAuthorSnapshot()}};
    emit draftChanged();
    saveDraft();
}

void MailBackend::finishGoogleSendSave(bool ok) {
    if (m_googleSendPending.isEmpty()) return;
    const auto pending = m_googleSendPending;
    const bool same = pending["account"] == m_account && pending["context"] == draftContext()
        && pending["generation"].toInt(-1) == m_generation && pending["authored"].toObject() == googleSendAuthorSnapshot();
    // Send was pressed while an earlier autosave was on its way: the save that
    // follows carries the version Send was pressed on.
    if (ok && same && m_draftDirty && !m_shutdown) return;
    m_googleSendPending = {};
    if (!ok || !same || m_draftDirty || m_shutdown) {
        m_googleSendUi[googleSendKey()].feedback = ok ? "The draft changed before sending. Review it and press Send again."
            : "Send was cancelled because the draft was not saved. Review it and press Send again.";
        emit draftChanged(); return;
    }
    const auto send = googleSend();
    if (!send.value("can_send").toBool() || m_draft.value("revision").toLongLong() <= 0
        || m_draft.value("draft_id").toString().isEmpty()) {
        m_googleSendUi[googleSendKey()].feedback = "Send is no longer available for this saved version. Check its current status.";
        emit draftChanged(); return;
    }
    // The background Gmail copy of this draft is finishing; Core sends once it
    // settles. Wait for it (a minute at most) instead of refusing.
    if (!send.value("send_now").toBool()) {
        m_googleSendPending = pending;
        if (!m_googleSendPending.contains("waiting_since")) m_googleSendPending["waiting_since"] = m_actionClock.elapsed();
        if (m_actionClock.elapsed() - m_googleSendPending["waiting_since"].toInteger() > 60000) {
            m_googleSendPending = {};
            m_googleSendUi[googleSendKey()].feedback = "Gmail is still saving this draft. Press Send again in a moment.";
        } else m_googleSendWaitTimer.start();
        emit draftChanged(); return;
    }
    m_googleSendWaitTimer.stop();
    // Name the newest Gmail-copy revision known from either status, so the
    // engine refuses the send if the copy has moved since.
    const auto params = QJsonObject{{"draft_id", m_draft.value("draft_id").toString()},
        {"expected_revision", m_draft.value("revision").toLongLong()},
        {"expected_google_revision", std::max(m_draft.value("google").toMap().value("revision").toLongLong(),
             m_draft.value("google_send").toMap().value("current_google_revision").toLongLong())},
        {"request_id", mutation()["request_id"]}};
    m_googleSendUi[googleSendKey()].selected = {};
    queueGoogleSend(params);
}

void MailBackend::refreshGoogleSendMetadata(const QString &account, const QString &draftId) {
    const auto generation = m_generation;
    request("draft_get", account, {{"draft_id", draftId}}, [this, account, draftId, generation](auto result, bool ok) {
        if (!ok || account != m_account || draftId != m_draft.value("draft_id").toString() || generation != m_generation
            || result["account"] != account || result["draft_id"] != draftId) return;
        mergeGoogleSendStatus(result["google_send"].toObject(), account, draftId);
        mergeGoogleDraftStatus(result["google"].toObject(), account, draftId);
        emit draftChanged();
    });
}

void MailBackend::startGoogleSendOperation(const QString &method, const QJsonObject &params) {
    if (draftReadOnly()) return;
    if ((accountArchived() || m_unsavedAccountBuffer) && method != "google_send_status") return;
    startGoogleSendForDraft(method, params, m_account, m_draft.value("draft_id").toString());
}

void MailBackend::startGoogleSendForDraft(const QString &method, const QJsonObject &params, const QString &account, const QString &draftId) {
    const auto key = account + '\n' + draftId;
    auto &ui = m_googleSendUi[key];
    if (ui.active) return;
    const bool freshAttempt = method == "google_send" && !ui.uncertain && ui.selected.isEmpty();
    if (method == "google_send") ui.request = params;
    else if (ui.request.isEmpty()) ui.request = QJsonObject::fromVariantMap(m_draft.value("google_send").toMap())["request"].toObject();
    ui.active = true; ui.method = method; ui.feedback.clear();
    const auto requestId = params["request_id"].toString();
    emit draftChanged();
    auto done = [this, account, draftId, key, requestId, method, params, freshAttempt](QJsonObject result, bool ok) {
        auto &saved = m_googleSendUi[key]; saved.active = false; saved.method.clear();
        const bool valid = ok && validSend(result, account, draftId) && result["request_id"] == requestId;
        if (valid) {
            if (saved.selected["request_id"] != requestId || result["revision"].toInteger() >= saved.selected["revision"].toInteger())
                saved.selected = result;
            saved.request = result["request"].toObject(); saved.uncertain = false; saved.notFound = false; saved.feedback.clear();
            if (result["operation_is_current"].toBool() && (saved.current.isEmpty() || !earlierSend(result, saved.current))) saved.current = result;
        } else if (!ok && method == "google_send" && saved.request == params
            && result["details"].toObject()["send_request_obsolete_without_dispatch"] == QJsonValue(true)) {
            // This stronger, exact-request Core proof includes all invocations:
            // no journal exists and monotonic revisions make every original
            // preparation permanently unable to claim a submission.
            saved.request = {}; saved.selected = {}; saved.uncertain = false; saved.notFound = false;
            saved.feedback = "The original request was never submitted and can no longer submit. Your newer edits remain here. Review them and press Send again.";
        } else if (!ok && freshAttempt && result["details"].toObject()["send_invocation_finished_before_dispatch"] == QJsonValue(true)) {
            // A completed direct refusal is scoped to this invocation. It can
            // release a new attempt, never an identity with a prior lost reply.
            saved.request = {}; saved.uncertain = false; saved.notFound = false;
            saved.feedback = "This attempt stopped before submission. " + result["message"].toString("Review the draft before sending again.");
        } else {
            // Anything else leaves the request unresolved. Nothing is resent
            // automatically; the user checks or resumes the same request.
            saved.uncertain = true;
            if (!ok && method == "google_send_status" && result["code"] == "not_found") saved.notFound = true;
            saved.feedback = "Result not confirmed. " + result["message"].toString("Check the exact saved request.");
        }
        if (method == "google_send") finishQueuedSendNotice(key, result, valid);
        const auto previewKey = account + '\n' + params.value("request_id").toString();
        if (m_transientOutgoing.contains(previewKey)) {
            if ((valid && (result["state"] == "rejected" || result["state"] == "abandoned")) || (!valid && !saved.uncertain)) m_transientOutgoing.remove(previewKey);
            else m_transientOutgoing[previewKey]["state"] = valid ? result["state"].toString() : "acceptance_unknown";
        }
        refreshOutgoingPresentation(); loadOutgoing();
        if (valid && result["state"] == "accepted") {
            loadAccounts();
            if (m_folder == "drafts") loadDraftRows(false, true);
        }
        if (account == m_account && draftId == m_draft.value("draft_id").toString()) {
            if (valid) mergeGoogleSendStatus(result, account, draftId);
            if (valid && !result["operation_is_current"].toBool()) refreshGoogleSendMetadata(account, draftId);
            if (googleSend().value("sent_current").toBool()) {
                m_draftStatus = "Sent"; remember("replyOpen", false);
            }
            emit draftChanged();
        }
    };
    // Status reads are local and use the main queue; send, reconcile and
    // abandon go through a dedicated operation connection.
    if (method == "google_send_status") request(method, account, params, std::move(done));
    else googleOperation(method, account, params, std::move(done));
}

void MailBackend::checkGoogleSend() {
    if (!googleSend().value("can_check").toBool()) return;
    const auto ui = m_googleSendUi.value(googleSendKey());
    const auto current = QJsonObject::fromVariantMap(m_draft.value("google_send").toMap());
    const bool otherRequest = !ui.request.isEmpty() && ui.request["request_id"] != current["request_id"];
    const auto selected = ui.selected.isEmpty() ? (otherRequest ? QJsonObject{} : current) : ui.selected;
    const auto id = selected["request_id"].toString(ui.request["request_id"].toString());
    const bool provider = !ui.uncertain && selected["state"] == "acceptance_unknown" && accountCapability(m_account, "send");
    startGoogleSendOperation(provider ? "google_send_reconcile" : "google_send_status", {{"request_id", id}});
}

void MailBackend::showCurrentGoogleSend() {
    if (!googleSend().value("can_select_current").toBool()) return;
    auto &ui = m_googleSendUi[googleSendKey()];
    ui.selected = QJsonObject::fromVariantMap(m_draft.value("google_send").toMap());
    ui.request = ui.selected["request"].toObject();
    ui.feedback.clear(); ui.uncertain = false; ui.notFound = false;
    emit draftChanged();
}

void MailBackend::resumeGoogleSend() {
    if (!googleSend().value("can_resume").toBool()) return;
    const auto ui = m_googleSendUi.value(googleSendKey());
    if (ui.uncertain && ui.notFound && ui.selected.isEmpty()) {
        startGoogleSendOperation("google_send", ui.request); return;
    }
    const auto selected = ui.selected.isEmpty() ? QJsonObject::fromVariantMap(m_draft.value("google_send").toMap()) : ui.selected;
    startGoogleSendOperation("google_send", selected["request"].toObject());
}

void MailBackend::abandonGoogleSend() {
    if (!googleSend().value("can_abandon").toBool()) return;
    const auto ui = m_googleSendUi.value(googleSendKey());
    const auto selected = ui.selected.isEmpty() ? QJsonObject::fromVariantMap(m_draft.value("google_send").toMap()) : ui.selected;
    startGoogleSendOperation("google_send_abandon", {{"request_id", selected["request_id"]}, {"expected_revision", selected["revision"]}});
}

void MailBackend::cancelGoogleSend() {
    if (sendQueued()) { cancelQueuedSend(googleSendKey(), true); return; }
    if (!m_googleSendPending.isEmpty()) { cancelPendingGoogleSend("Send cancelled before submission."); return; }
    const auto ui = m_googleSendUi.value(googleSendKey());
    if (!ui.active) return;
    const auto operations = m_googleOperations;
    for (auto operation : operations) if (operation->account() == m_account && operation->method() == ui.method
        && operation->requestId() == ui.request["request_id"].toString()) operation->cancel();
}
