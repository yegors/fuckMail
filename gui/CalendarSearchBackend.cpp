#include "MailBackend.h"
#include "CalendarLayout.h"
#include <QDate>
#include <QJsonArray>
#include <QJsonDocument>
#include <QTimeZone>

namespace {
QString identity(const QVariantMap &row) {
    return QString::fromUtf8(QJsonDocument(QJsonArray{row.value("account").toString(),
        row.value("calendar_id").toString(), row.value("id").toString(),
        QJsonValue::fromVariant(row.value("occurrence_key"))}).toJson(QJsonDocument::Compact));
}
}

void MailBackend::initializeCalendarSearch() {
    m_calendarSearchTimer.setSingleShot(true);
    m_calendarSearchTimer.setInterval(100);
    connect(&m_calendarSearchTimer, &QTimer::timeout, this, &MailBackend::runCalendarSearch);
    for (auto signal : {&MailBackend::eventsChanged, &MailBackend::accountChanged,
            &MailBackend::accountsChanged, &MailBackend::accountLifecycleChanged,
            &MailBackend::preferencesChanged, &MailBackend::connectedChanged})
        connect(this, signal, this, &MailBackend::refreshCalendarSearchContext);
}

QJsonObject MailBackend::calendarSearchParams() const {
    QJsonValue selection = effectiveCalendarSelection();
    if (!m_calendarSearchCombined) {
        QJsonArray selected;
        const auto candidates = selection.isArray() ? selection.toArray()
            : QJsonArray::fromVariantList(m_calendarSources);
        for (const auto &value : candidates) {
            const auto source = value.toObject();
            if (source["account"].toString() == m_account)
                selected.append(QJsonObject{{"account", m_account}, {"calendar_id", source["calendar_id"]}});
        }
        selection = selected;
    }
    const auto from = calendarFrom();
    return {{"from", from}, {"to", QDate::fromString(from, Qt::ISODate).addDays(calendarDays()).toString(Qt::ISODate)},
        {"calendars", selection}};
}

QString MailBackend::calendarSearchContextStamp() const {
    QJsonArray accounts;
    for (const auto &value : m_allAccounts) {
        const auto row = value.toMap();
        accounts.append(QJsonObject{{"id", row.value("id").toString()},
            {"lifecycle", QJsonValue::fromVariant(row.value("lifecycle"))},
            {"google", QJsonValue::fromVariant(row.value("google"))}});
    }
    return QString::fromUtf8(QJsonDocument(QJsonObject{{"params", calendarSearchParams()},
        {"query", m_calendarSearchQuery}, {"active", m_calendarSearchActive}, {"combined", m_calendarSearchCombined},
        {"account", m_account}, {"accounts", accounts}, {"sources", QJsonArray::fromVariantList(m_calendarSources)},
        {"zone", m_preferences.value("calendar_display_time_zone").toString()},
        {"generation", m_calendarGeneration}, {"revision", m_calendarRevision}, {"connected", m_connected},
        {"shutdown", m_shutdown}}).toJson(QJsonDocument::Compact));
}

QVariantMap MailBackend::calendarSearch() const {
    auto result = m_calendarSearchResult;
    result["events"] = visibleCalendarEvents(result.value("events").toList());
    const auto params = calendarSearchParams();
    result["from"] = params["from"].toString(); result["to"] = params["to"].toString();
    result["busy"] = m_calendarSearchBusy; result["status"] = m_calendarSearchStatus;
    return result;
}

void MailBackend::setCalendarSearchContext(bool active, bool combined) {
    m_calendarSearchActive = active; m_calendarSearchCombined = combined;
    refreshCalendarSearchContext();
}
void MailBackend::setCalendarSearchQuery(const QString &query) {
    if (m_calendarSearchQuery == query) return;
    m_calendarSearchQuery = query;
    refreshCalendarSearchContext();
    if (!m_calendarSearchActive) emit calendarSearchChanged();
}
void MailBackend::refreshCalendarSearchContext() {
    // Mail navigation must not serialize the Calendar catalog when this popup
    // is idle. A deactivation still invalidates every held search/selection.
    if (!m_calendarSearchActive) {
        if (m_calendarSearchContext.isEmpty()) return;
        ++m_calendarSearchGeneration; m_calendarSearchContext.clear(); m_calendarSearchTimer.stop();
        m_calendarSearchResult.clear(); m_calendarSearchBusy = false; m_calendarSearchStatus.clear();
        emit calendarSearchChanged(); return;
    }
    const auto stamp = calendarSearchContextStamp();
    if (stamp == m_calendarSearchContext) return;
    m_calendarSearchContext = stamp; ++m_calendarSearchGeneration;
    m_calendarSearchTimer.stop(); m_calendarSearchResult.clear(); m_calendarSearchBusy = false;
    m_calendarSearchStatus = "Enter words to search this date range";
    if (m_calendarSearchActive && !m_calendarSearchQuery.trimmed().isEmpty()) {
        if (!m_connected || m_shutdown) m_calendarSearchStatus = "Connect to the local engine to search saved events";
        else { m_calendarSearchBusy = true; m_calendarSearchStatus = "Searching cached events…"; m_calendarSearchTimer.start(); }
    }
    emit calendarSearchChanged();
}

void MailBackend::runCalendarSearch() {
    if (!m_calendarSearchActive || !m_connected || m_shutdown) return;
    const auto generation = m_calendarSearchGeneration;
    const auto stamp = calendarSearchContextStamp();
    auto params = calendarSearchParams(); params["query"] = m_calendarSearchQuery; params["limit"] = 100;
    request("calendar_search", {}, params, [this, generation, stamp, params](auto result, bool ok) {
        if (generation != m_calendarSearchGeneration || stamp != calendarSearchContextStamp()) return;
        m_calendarSearchBusy = false;
        if (!ok) {
            m_calendarSearchStatus = result["message"].toString("Calendar search is unavailable. Edit the query to retry.");
        } else if (result["query"] != params["query"] || result["from"] != params["from"] || result["to"] != params["to"]
            || !result["events"].isArray() || result["events"].toArray().size() > 100
            || (!QTimeZone(result["time_zone"].toString().toUtf8()).isValid()
                && result["coverage"].toObject()["state"] != "display_zone_required")) {
            m_calendarSearchStatus = "Calendar search returned a different context. Edit the query to retry.";
        } else {
            // Reuse the normal date/time presentation. Keep one row per shared
            // occurrence even when its display spans more than one civil day.
            const auto from = QDate::fromString(params["from"].toString(), Qt::ISODate);
            const auto to = QDate::fromString(params["to"].toString(), Qt::ISODate);
            const auto presented = calendarPresentation(result["events"].toArray(), from, int(from.daysTo(to)),
                QTimeZone(result["time_zone"].toString().toUtf8()));
            QVariantList rows; QSet<QString> seen;
            for (const auto &value : presented) {
                const auto row = value.toMap(); const auto key = identity(row);
                if (seen.contains(key)) continue;
                seen.insert(key); rows.append(row);
            }
            m_calendarSearchResult = result.toVariantMap(); m_calendarSearchResult["events"] = rows;
            m_calendarSearchStatus = calendarCoverageText(result["coverage"].toObject());
        }
        emit calendarSearchChanged();
    });
}

void MailBackend::openCalendarSearchResult(const QVariantMap &event) {
    if (!m_calendarSearchActive || !m_connected || m_shutdown || m_calendarSearchBusy
        || m_calendarSearchContext != calendarSearchContextStamp()
        || !m_calendarSearchResult.value("events").toList().contains(event) || calendarEventHidden(event)) return;
    const auto date = QDate::fromString(event.value("date").toString(), Qt::ISODate);
    if (!date.isValid()) return;
    m_calendarSearchTimer.stop(); const int generation = ++m_calendarSearchGeneration;
    const auto stamp = calendarSearchContextStamp();
    auto params = calendarSearchParams();
    params["from"] = date.toString(Qt::ISODate); params["to"] = date.addDays(1).toString(Qt::ISODate);
    // Search may narrow further, but opening a match retains the Calendar's
    // saved visibility and individual source selection.
    params["calendars"] = effectiveCalendarSelection();
    m_calendarSearchBusy = true; m_calendarSearchStatus = "Opening the current saved event…"; emit calendarSearchChanged();
    request("calendar_combined", {}, params, [this, event, date, generation, stamp, params](auto result, bool ok) {
        if (generation != m_calendarSearchGeneration || stamp != calendarSearchContextStamp()) return;
        m_calendarSearchBusy = false;
        QVariantList fresh;
        if (ok && result["from"] == params["from"] && result["to"] == params["to"])
            fresh = calendarPresentation(result["events"].toArray(), date, 1, QTimeZone(result["time_zone"].toString().toUtf8()));
        QVariantMap selected;
        for (const auto &value : fresh) {
            const auto row = value.toMap();
            if (identity(row) == identity(event) && !row.value("cancelled").toBool() && !row.value("busy_only").toBool()) { selected = row; break; }
        }
        if (selected.isEmpty() || calendarEventHidden(selected)) {
            m_calendarSearchResult.clear();
            m_calendarSearchStatus = "This event changed or is no longer visible on that date. Search again.";
            emit calendarSearchChanged(); return;
        }
        // Publish only the fresh shared query, using the same presentation as
        // loadCalendar. Fence older window reads before navigating to this day.
        ++m_calendarGeneration; m_calendarView = "day"; m_calendarDate = date.toString(Qt::ISODate);
        m_weekStart = m_calendarDate; publishCalendarWindow(result, m_calendarDate, 1);
        m_calendarScope = QString::fromUtf8(QJsonDocument(params).toJson(QJsonDocument::Compact)) + "/"
            + m_preferences.value("calendar_display_time_zone").toString();
        setPreferences({{"calendar_view", "day"}, {"calendar_date", m_calendarDate}});
        emit eventsChanged();
        emit calendarSearchResultReady(selected);
    });
}
