#include "MailBackend.h"
#include <QJsonArray>
#include <memory>

void MailBackend::loadCalendarOperations() {
    m_calendarOperationStatus.clear();
    loadCalendarOperationsPage(false);
}

void MailBackend::loadMoreCalendarOperations() {
    if (!m_calendarOperationsLoading && !m_calendarOperationCursors.isEmpty()) loadCalendarOperationsPage(true);
}

void MailBackend::loadCalendarOperationsPage(bool append) {
    const int generation = ++m_calendarOperationsGeneration;
    if (!append) m_calendarOperationCursors.clear();
    if (!m_connected) {
        m_calendarOperationsLoading = false;
        m_calendarOperationStatus = "Reconnect to inspect calendar activity.";
        emit calendarOperationsChanged(); return;
    }
    m_calendarOperationsLoading = true; emit calendarOperationsChanged();
    auto collected = std::make_shared<QVariantList>(append ? m_calendarOperations : QVariantList());
    auto remaining = std::make_shared<int>(0);
    for (const auto &entry : m_accounts) {
        if (!append || m_calendarOperationCursors.contains(entry.toMap()["id"].toString())) ++*remaining;
    }
    auto error = std::make_shared<QString>();
    if (!*remaining) { m_calendarOperations.clear(); m_calendarOperationsLoading = false; emit calendarOperationsChanged(); return; }
    for (const auto &entry : m_accounts) {
        const auto account = entry.toMap();
        const auto id = account["id"].toString(), name = account["name"].toString();
        if (append && !m_calendarOperationCursors.contains(id)) continue;
        QJsonObject params{{"limit", 50}};
        if (append) params["cursor"] = m_calendarOperationCursors[id];
        request("calendar_operations", id, params, [this, generation, id, name, collected, remaining, error](auto result, bool ok) {
            if (generation != m_calendarOperationsGeneration) return;
            if (ok) {
                if (result["continuation"].isObject()) m_calendarOperationCursors[id] = result["continuation"].toObject();
                else m_calendarOperationCursors.remove(id);
                for (const auto &operation : result["operations"].toArray()) {
                    auto row = operation.toObject().toVariantMap();
                    row["account"] = id; row["account_name"] = name;
                    bool replaced = false;
                    for (auto &existing : *collected) {
                        const auto previous = existing.toMap();
                        if (previous["account"].toString() == id && previous["request_id"] == row["request_id"]) { existing = row; replaced = true; break; }
                    }
                    if (!replaced) collected->append(row);
                }
            } else *error = "Some operation history is unavailable. " + result["message"].toString();
            if (--*remaining == 0) {
                m_calendarOperations = *collected; m_calendarOperationsLoading = false;
                if (!error->isEmpty()) m_calendarOperationStatus = *error;
                emit calendarOperationsChanged();
            }
        });
    }
}

void MailBackend::retryCalendarOperation(const QString &account, const QString &requestId) {
    if (m_calendarOperationsLoading || !m_connected) return;
    bool recorded = false;
    QString state;
    for (const auto &operation : m_calendarOperations) {
        const auto row = operation.toMap();
        if (row["account"].toString() == account && row["request_id"].toString() == requestId
            && row["state"].toString() != "fake_acknowledged") { recorded = true; state = row["state"].toString(); break; }
    }
    if (!recorded) return;
    if (!m_syntheticProfile) {
        calendarOperationAction(account, requestId, state == "prepared" || state == "ready" ? "resume"
            : state == "unknown" || state == "accepted" ? "reconcile" : "check");
        return;
    }
    m_calendarOperationsLoading = true; ++m_calendarWrites;
    m_calendarOperationStatus = "Checking the original local operation…"; emit calendarOperationsChanged();
    request("calendar_write_ack", account, {{"request_id", requestId}}, [this](auto result, bool ok) {
        --m_calendarWrites; m_calendarOperationsLoading = false;
        if (ok) {
            m_calendarOperationStatus = "Saved locally. The original operation was reused; no invitations were sent.";
            loadCalendar(); loadCalendarSources();
        } else {
            cancelShutdown();
            m_calendarOperationStatus = result["message"].toString("Outcome is not confirmed. Retry checks the same recorded operation.");
        }
        emit calendarOperationsChanged(); loadCalendarOperationsPage(false);
        if (m_shutdown) finishShutdown();
    });
}

void MailBackend::calendarOperationAction(const QString &account, const QString &requestId, const QString &action, qint64 expectedRevision) {
    if (m_syntheticProfile) {
        if (action == "resume" || action == "check" || action == "reconcile") retryCalendarOperation(account, requestId);
        return;
    }
    const QString method = action == "reconcile" ? "google_calendar_reconcile"
        : action == "resume" ? "google_calendar_resume"
        : action == "abandon" ? "google_calendar_abandon"
        : action == "check" ? "google_calendar_operation" : QString{};
    ++m_calendarWrites;
    if (method.isEmpty() || account.isEmpty() || requestId.isEmpty() || (action == "abandon" && expectedRevision < 0)) {
        finishCalendarOperation(account, requestId, {{"code", "invalid_request"}, {"message", "Choose a recorded calendar change."}}, false); return;
    }
    if (!m_connected || m_shutdown) {
        finishCalendarOperation(account, requestId, {{"code", "calendar_unavailable"}, {"message", "Reconnect to inspect the recorded change."}}, false); return;
    }
    if ((action == "resume" || action == "reconcile")
        && (!accountCapability(account, "calendar_write") || !googleSyncState(account).value("active").toBool())) {
        finishCalendarOperation(account, requestId, {{"code", "calendar_sync_required"}, {"message", "Start Calendar sync for this Google account before continuing its change."}}, false); return;
    }
    QJsonObject params{{"request_id", requestId}};
    if (action == "abandon") params["expected_revision"] = expectedRevision;
    m_calendarOperationsLoading = true;
    m_calendarOperationStatus = action == "resume" ? "Resuming the recorded change…"
        : action == "abandon" ? "Abandoning the recorded change…" : "Checking the recorded change…";
    emit calendarOperationsChanged();
    const auto completed = [this, account, requestId, action](auto result, bool ok) {
        result["operation_action"] = action;
        finishCalendarOperation(account, requestId, result, ok);
    };
    if (action == "reconcile" || action == "resume") googleOperation(method, account, params, completed);
    else request(method, account, params, completed);
}

void MailBackend::finishCalendarOperation(const QString &account, const QString &requestId, QJsonObject result, bool ok) {
    --m_calendarWrites;
    const auto returnedId = result["request_id"].toString();
    const auto returnedAccount = result["target"].toObject()["account"].toString();
    if ((!returnedId.isEmpty() && returnedId != requestId) || (!returnedAccount.isEmpty() && returnedAccount != account)) {
        ok = false; result = {{"code", "calendar_response_invalid"}, {"message", "The calendar response did not match this recorded change."}};
    }
    result["account"] = account; result["request_id"] = requestId;
    const auto state = result["state"].toString();
    const bool confirmed = ok && (m_syntheticProfile || state == "accepted" || state == "acknowledged");
    QString message;
    if (!ok) message = result["message"].toString("The calendar outcome could not be confirmed. Your change remains available.");
    else if (m_syntheticProfile) message = "Saved locally. No invitations were sent.";
    else if (state == "acknowledged") message = "Saved to Google Calendar.";
    else if (state == "accepted") message = "Google confirmed the change. The local calendar is refreshing.";
    else if (state == "ready") message = "The change is ready but has not been sent. Resume sends this same change.";
    else if (state == "prepared") message = "The change is recorded but has not been sent. Resume sends this same change.";
    else if (state == "in_flight") message = "The recorded change is still in progress. Check its status before taking another action.";
    else if (state == "unknown") message = "Google's outcome is not confirmed. Check the outcome to reconcile this same change.";
    else if (state == "conflict") message = "The event changed on Google. Your submitted fields are retained; review the current version.";
    else if (state == "rejected") message = "Google did not accept this change. Your submitted fields are retained.";
    else if (state == "abandoned") message = "The recorded change was abandoned. This does not undo a change already accepted by Google.";
    else message = "The calendar outcome needs review in activity.";
    if (!confirmed && state != "abandoned" && !(m_shutdown && result["code"].toString() == "cancelled")) cancelShutdown();
    m_calendarOperationStatus = message;
    m_calendarOperationsLoading = false;
    emit calendarWriteFinished(confirmed, result.toVariantMap(), confirmed ? QString{} : message);
    emit calendarOperationsChanged();
    if (confirmed) { loadCalendar(); loadCalendarSources(); }
    if (!m_syntheticProfile) loadCalendarOperationsPage(false);
    if (m_shutdown) finishShutdown();
}
