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
        m_calendarOperationStatus = "Reconnect to inspect local calendar operations.";
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
    for (const auto &operation : m_calendarOperations) {
        const auto row = operation.toMap();
        if (row["account"].toString() == account && row["request_id"].toString() == requestId
            && row["state"].toString() != "fake_acknowledged") { recorded = true; break; }
    }
    if (!recorded) return;
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
