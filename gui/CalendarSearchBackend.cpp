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
    m_calendarMailActivityTimer.setInterval(2000);
    connect(&m_calendarMailActivityTimer, &QTimer::timeout, this, &MailBackend::refreshCalendarMailActivity);
    m_calendarSearchTimer.setSingleShot(true);
    m_calendarSearchTimer.setInterval(100);
    connect(&m_calendarSearchTimer, &QTimer::timeout, this, &MailBackend::runCalendarSearch);
    for (auto signal : {&MailBackend::eventsChanged, &MailBackend::calendarSourcesChanged, &MailBackend::accountChanged,
            &MailBackend::accountsChanged, &MailBackend::accountLifecycleChanged,
            &MailBackend::preferencesChanged, &MailBackend::connectedChanged})
    {
        connect(this, signal, this, &MailBackend::refreshCalendarSearchContext);
        // Invalidate immediately on navigation/permission changes. Revisions
        // alone refresh at most once per timer interval while Calendar is open.
        connect(this, signal, this, [this] {
            if (!m_calendarMailActivityActive) return;
            refreshCalendarMailActivity();
        });
    }
}

void MailBackend::setCalendarMailActivityContext(bool active, bool combined) {
    if (active == m_calendarMailActivityActive && combined == m_calendarMailActivityCombined) { refreshCalendarMailActivity(); return; }
    m_calendarMailActivityActive = active; m_calendarMailActivityCombined = combined;
    if (active) { m_calendarMailActivityTimer.start(); refreshCalendarMailActivity(); m_calendarPrefetchTimer.start(); }
    else {
        m_calendarMailActivityTimer.stop(); ++m_calendarMailActivityGeneration;
        m_calendarMailActivityBusy = false; m_calendarMailActivityScope.clear(); m_calendarMailActivityStamp.clear();
        m_calendarMailActivity.clear(); emit calendarMailActivityChanged();
    }
}

void MailBackend::refreshCalendarMailActivity() {
    if (!m_calendarMailActivityActive || m_shutdown) return;
    // Counts cover the whole month window, so day/week/month switches reuse them.
    const auto window = calendarBaseWindow(m_calendarDate);
    const auto from = window.from;
    const auto to = QDate::fromString(from, Qt::ISODate).addDays(window.days).toString(Qt::ISODate);
    const auto account = m_calendarMailActivityCombined ? QString{} : m_account;
    QJsonArray accounts;
    for (const auto &entry : m_accounts) accounts.append(entry.toMap().value("id").toString());
    const auto scope = QString::fromUtf8(QJsonDocument(QJsonArray{from, to, account, accounts,
        m_preferences.value("calendar_display_time_zone").toString(), m_connected}).toJson(QJsonDocument::Compact));
    const bool newScope = scope != m_calendarMailActivityScope;
    if (newScope) {
        m_calendarMailActivityScope = scope; m_calendarMailActivityStamp.clear(); ++m_calendarMailActivityGeneration;
        m_calendarMailActivityBusy = false;
        m_calendarMailActivity = {{"state", "loading"}, {"from", from}, {"to", to}};
        emit calendarMailActivityChanged();
    }
    if (!m_connected || (!m_calendarMailActivityCombined && account.isEmpty())) {
        m_calendarMailActivity = {{"state", "unavailable"}, {"message", "Mail counts are waiting for the local engine and account."}};
        emit calendarMailActivityChanged(); return;
    }
    // Only mail changes move counts; calendar and model writes do not.
    const auto stamp = scope + '/' + m_diagnostics.value("mail_generation", m_diagnostics.value("revision")).toString();
    if (m_calendarMailActivityBusy || stamp == m_calendarMailActivityStamp) return;
    // Signals can arrive on every sync page. Keep repeated aggregate reads off
    // the foreground queue; a changed date/account still loads immediately.
    if (!newScope && sender() != &m_calendarMailActivityTimer) return;
    m_calendarMailActivityBusy = true; m_calendarMailActivityStamp = stamp;
    const auto generation = m_calendarMailActivityGeneration;
    const auto zone = m_preferences.value("calendar_display_time_zone").toString();
    request("calendar_mail_activity", account, {{"from", from}, {"to", to}}, [this, generation, from, to, account, zone](auto result, bool ok) {
        if (generation != m_calendarMailActivityGeneration || !m_calendarMailActivityActive || m_shutdown) return;
        m_calendarMailActivityBusy = false;
        const auto rows = result["days"].toArray();
        const auto first = QDate::fromString(from, Qt::ISODate);
        const bool zoneRequired = result["state"] == "display_zone_required" && result["time_zone"].isNull() && rows.isEmpty();
        bool valid = zoneRequired || (result["state"] == "ready" && result["time_zone"] == zone && rows.size() == first.daysTo(QDate::fromString(to, Qt::ISODate)));
        for (qsizetype i = 0; i < rows.size() && valid; ++i) {
            const auto day = rows[i].toObject();
            valid = day["date"] == first.addDays(i).toString(Qt::ISODate);
            for (const auto field : {"received", "sent"}) valid = valid && day[field].isDouble() && day[field].toInteger(-1) >= 0 && day[field].toDouble() == double(day[field].toInteger(-1));
        }
        if (!ok || !valid || result["from"] != from || result["to"] != to || !result["days"].isArray()
            || (account.isEmpty() ? !result["account"].isNull() : result["account"] != account)) {
            m_calendarMailActivity = {{"state", "unavailable"}, {"message", result["message"].toString("Mail counts could not load.")}};
        } else {
            m_calendarMailActivity = result.toVariantMap();
            // Each day badge looks its row up directly instead of searching the list.
            QVariantMap byDate;
            for (const auto &row : rows) byDate.insert(row.toObject()["date"].toString(), row.toObject().toVariantMap());
            m_calendarMailActivity["by_date"] = byDate;
        }
        emit calendarMailActivityChanged();
    });
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
    const auto epoch = m_calendarCacheEpoch;
    request("calendar_combined", {}, params, [this, event, date, generation, stamp, params, epoch](auto result, bool ok) {
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
        // Show exactly this fresh read: it is cached under the visible day's own key.
        m_calendarView = "day"; m_calendarDate = date.toString(Qt::ISODate);
        if (epoch == m_calendarCacheEpoch) storeCalendarWindow(calendarWindow(m_calendarDate, 1).key, result);
        rememberCalendarNavigation({{"calendar_view", "day"}, {"calendar_date", m_calendarDate}});
        showCalendarRange();
        emit calendarSearchResultReady(selected);
    });
}
