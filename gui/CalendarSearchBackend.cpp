// MailBackend's Calendar search over cached events in the selected calendars:
// the dropdown's next matches, the filter it puts on the Calendar views and its
// Recent list. Also the per-day mail counts on the calendar grid.
#include "MailBackend.h"
#include "CalendarLayout.h"
#include "SearchMatch.h"
#include <QDate>
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QTimeZone>
#include <algorithm>

namespace {
// One occurrence's identity: account, calendar, event id and occurrence key.
// Collapses the per-day segments of one event and finds it again after a re-read.
QString identity(const QVariantMap &row) {
    return QString::fromUtf8(QJsonDocument(QJsonArray{row.value("account").toString(),
        row.value("calendar_id").toString(), row.value("id").toString(),
        QJsonValue::fromVariant(row.value("occurrence_key"))}).toJson(QJsonDocument::Compact));
}
}

void MailBackend::initializeCalendarSearch() {
    m_calendarMailActivityTimer.setInterval(2000);
    connect(&m_calendarMailActivityTimer, &QTimer::timeout, this, &MailBackend::refreshCalendarMailActivity);
    // Debounces typing: one search per 100 ms pause.
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
    // Matches run from today, whatever day the Calendar shows.
    return {{"around", todayDate()}, {"calendars", selection}};
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
        {"revision", m_calendarRevision}, {"connected", m_connected},
        {"shutdown", m_shutdown}}).toJson(QJsonDocument::Compact));
}

QVariantMap MailBackend::calendarSearch() const {
    auto result = m_calendarSearchResult;
    result["events"] = visibleCalendarEvents(result.value("events").toList());
    result["busy"] = m_calendarSearchBusy; result["status"] = m_calendarSearchStatus;
    return result;
}
QVariantMap MailBackend::calendarSearchTerms() const {
    return m_calendarSearchQuery.trimmed().isEmpty() ? QVariantMap{} : m_calendarSearchTerms;
}

void MailBackend::setCalendarSearchContext(bool active, bool combined) {
    m_calendarSearchActive = active; m_calendarSearchCombined = combined;
    refreshCalendarSearchContext();
}
void MailBackend::setCalendarSearchQuery(const QString &query) {
    if (m_calendarSearchQuery == query) return;
    m_calendarSearchQuery = query;
    // Clearing stops the filter at once. Other text keeps the last reading
    // until Core reads the new one, so the views do not blink per key.
    if (query.trimmed().isEmpty()) { m_calendarSearchTerms.clear(); m_calendarSearchCommit.clear(); }
    refreshCalendarSearchContext();
    if (!m_calendarSearchActive) emit calendarSearchChanged();
}
void MailBackend::refreshCalendarSearchContext() {
    // Mail navigation must not serialize the Calendar catalog when search
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
    m_calendarSearchStatus.clear();
    if (!m_calendarSearchQuery.trimmed().isEmpty()) {
        if (!m_connected || m_shutdown) m_calendarSearchStatus = "Connect to the local engine to search saved events";
        else { m_calendarSearchBusy = true; m_calendarSearchTimer.start(); }
    }
    emit calendarSearchChanged();
}

void MailBackend::runCalendarSearch() {
    if (!m_calendarSearchActive || !m_connected || m_shutdown) return;
    const auto generation = m_calendarSearchGeneration;
    const auto stamp = calendarSearchContextStamp();
    auto params = calendarSearchParams(); params["query"] = m_calendarSearchQuery; params["limit"] = 7;
    request("calendar_search", {}, params, [this, generation, stamp, params](auto result, bool ok) {
        // Any change to the query, selection, accounts or cached calendar
        // while the search ran makes its rows stale; drop them silently.
        if (generation != m_calendarSearchGeneration || stamp != calendarSearchContextStamp()) return;
        m_calendarSearchBusy = false;
        const QTimeZone zone(result["time_zone"].toString().toUtf8());
        const auto coverage = result["coverage"].toObject();
        const bool zoneRequired = coverage["state"] == "display_zone_required";
        if (!ok) {
            m_calendarSearchStatus = result["message"].toString("Calendar search is unavailable. Edit the query to retry.");
        } else if (result["query"] != params["query"] || result["around"] != params["around"]
            || !result["events"].isArray() || result["events"].toArray().size() > 7 || !result["terms"].isObject()
            || (!zone.isValid() && !zoneRequired)) {
            m_calendarSearchStatus = "Calendar search returned a different context. Edit the query to retry.";
        } else {
            // Each match shows on its own day, in Core's order: upcoming
            // first, then the recent past. Ongoing events show today.
            const auto today = QDate::fromString(params["around"].toString(), Qt::ISODate);
            QVariantList rows;
            for (const auto &value : result["events"].toArray()) {
                const auto row = value.toObject();
                const auto day = row["all_day"].toBool() ? QDate::fromString(row["start"].toString(), Qt::ISODate)
                    : QDateTime::fromString(row["start"].toString(), Qt::ISODate).toTimeZone(zone).date();
                QVariantList shown;
                if (!row["past"].toBool() && day < today) shown = calendarPresentation({row}, today, 1, zone);
                if (shown.isEmpty()) shown = calendarPresentation({row}, day, 1, zone);
                if (!shown.isEmpty()) rows.append(shown.first());
            }
            m_calendarSearchResult = result.toVariantMap(); m_calendarSearchResult["events"] = rows;
            m_calendarSearchTerms = result["terms"].toObject().toVariantMap();
            m_calendarSearchStatus = zoneRequired || (!m_syntheticProfile && coverage["selected_calendars"].toInt() == 0)
                ? calendarCoverageText(coverage) : QString();
            if (!m_calendarSearchCommit.isEmpty() && m_calendarSearchCommit == m_calendarSearchQuery)
                applyCalendarSearchCommit(m_calendarSearchTerms);
        }
        emit calendarSearchChanged();
    });
}

QVariantList MailBackend::filterCalendarEvents(const QVariantList &rows, const QVariantMap &terms) const {
    const auto words = terms.value("words").toStringList(), titles = terms.value("title").toStringList();
    const auto people = terms.value("with").toStringList();
    if (words.isEmpty() && titles.isEmpty() && people.isEmpty()) return rows;
    // Core's rules, as mail search uses: folded, from a word start, the last word may be partial.
    const auto has = [](const QString &text, const QString &phrase) { return !SearchMatch::ranges(text, {phrase}).isEmpty(); };
    QVariantList kept;
    for (const auto &value : rows) {
        const auto row = value.toMap();
        if (row.value("busy_only").toBool()) continue;
        const auto title = row.value("title").toString(), notes = row.value("description").toString();
        bool match = std::all_of(titles.cbegin(), titles.cend(), [&](const auto &phrase) { return has(title, phrase); })
            && std::all_of(words.cbegin(), words.cend(), [&](const auto &phrase) { return has(title, phrase) || has(notes, phrase); });
        if (match && !people.isEmpty()) {
            QStringList fields{row.value("organizer").toString(), row.value("organizer_name").toString()};
            for (const auto &attendee : row.value("attendees").toList())
                for (const auto key : {"email", "displayName", "name"}) fields << attendee.toMap().value(key).toString();
            match = std::all_of(people.cbegin(), people.cend(), [&](const auto &person) {
                return std::any_of(fields.cbegin(), fields.cend(), [&](const auto &field) { return has(field, person); });
            });
        }
        if (match) kept.append(value);
    }
    return kept.size() == rows.size() ? rows : reflowCalendarPresentation(kept);
}

QString MailBackend::calendarQueryDate(const QString &value) const {
    const auto text = value.trimmed().toLower();
    const auto today = QDate::fromString(todayDate(), Qt::ISODate);
    if (text.isEmpty() || !today.isValid()) return {};
    static const QRegularExpression offset(R"(^([+-]\d{1,4})d?$)"), full(R"(^(\d{4})-(\d{1,2})-(\d{1,2})$)"),
        monthDay(R"(^(\d{1,2})-(\d{1,2})$)");
    QDate day;
    if (text == u"today") day = today;
    else if (text == u"tomorrow") day = today.addDays(1);
    else if (text == u"yesterday") day = today.addDays(-1);
    else if (const auto match = offset.match(text); match.hasMatch()) day = today.addDays(match.captured(1).toInt());
    else if (const auto match = full.match(text); match.hasMatch()) day = QDate(match.captured(1).toInt(), match.captured(2).toInt(), match.captured(3).toInt());
    else if (const auto match = monthDay.match(text); match.hasMatch()) day = QDate(today.year(), match.captured(1).toInt(), match.captured(2).toInt());
    return day.isValid() ? day.toString(Qt::ISODate) : QString();
}

void MailBackend::rememberCalendarSearch(const QString &text) {
    static const QRegularExpression control(QStringLiteral("[\\x{0}-\\x{1f}\\x{7f}-\\x{9f}]"));
    const auto value = text.trimmed();
    if (value.isEmpty() || value.toUtf8().size() > 512 || value.contains(control)) return;
    auto recent = calendarRecentSearches();
    recent.removeAll(value); recent.prepend(value);
    while (recent.size() > 6) recent.removeLast();
    if (recent != calendarRecentSearches()) setPreferences({{"calendar_recent_searches", recent}});
}
void MailBackend::removeCalendarRecentSearch(const QString &text) {
    auto recent = calendarRecentSearches();
    if (recent.removeAll(text)) setPreferences({{"calendar_recent_searches", recent}});
}
void MailBackend::commitCalendarSearch(const QString &text) {
    const auto value = text.trimmed();
    if (value.isEmpty()) return;
    rememberCalendarSearch(value);
    m_calendarSearchCommit = value;
    setCalendarSearchQuery(value);
    // Otherwise it follows once Core has read this exact text.
    if (m_calendarSearchTerms.value("text").toString() == value) applyCalendarSearchCommit(m_calendarSearchTerms);
}
void MailBackend::applyCalendarSearchCommit(const QVariantMap &terms) {
    const auto text = m_calendarSearchCommit; m_calendarSearchCommit.clear();
    const bool words = !terms.value("words").toList().isEmpty() || !terms.value("title").toList().isEmpty();
    const bool people = !terms.value("with").toList().isEmpty();
    const auto date = calendarQueryDate(terms.value("date").toString());
    if (!date.isEmpty()) {
        // date: moves to that day; the rest of the search keeps filtering.
        static const QRegularExpression token(R"((^|\s)date:\S*)", QRegularExpression::CaseInsensitiveOption);
        const auto rest = QString(text).replace(token, QStringLiteral(" ")).simplified();
        if (words || people) { auto kept = terms; kept["date"] = QVariant(); kept["text"] = rest; m_calendarSearchTerms = kept; }
        setCalendarSearchQuery(words || people ? rest : QString());
        openCalendarDay(date);
        return;
    }
    // with: alone shows this month with them.
    if (people && !words && (m_calendarView != "month" || m_calendarDate != todayDate())) {
        m_calendarView = "month"; m_calendarDate = todayDate();
        rememberCalendarNavigation({{"calendar_view", "month"}, {"calendar_date", m_calendarDate}}); showCalendarRange();
    }
}

void MailBackend::openCalendarSearchResult(const QVariantMap &event) {
    if (!m_calendarSearchActive || !m_connected || m_shutdown || m_calendarSearchBusy
        || m_calendarSearchContext != calendarSearchContextStamp()
        || !m_calendarSearchResult.value("events").toList().contains(event) || calendarEventHidden(event)) return;
    const auto date = QDate::fromString(event.value("date").toString(), Qt::ISODate);
    if (!date.isValid()) return;
    // Re-read that day rather than trusting the search row, so the event
    // opens with its current details and is refused if it has since changed.
    m_calendarSearchTimer.stop(); const int generation = ++m_calendarSearchGeneration;
    const auto stamp = calendarSearchContextStamp();
    // Search may narrow further, but opening a match retains the Calendar's
    // saved visibility and individual source selection.
    const QJsonObject params{{"from", date.toString(Qt::ISODate)}, {"to", date.addDays(1).toString(Qt::ISODate)},
        {"calendars", effectiveCalendarSelection()}};
    m_calendarSearchBusy = true; m_calendarSearchStatus = "Opening the current saved event…"; emit calendarSearchChanged();
    const auto epoch = m_calendarCacheEpoch;
    request("calendar_combined", {}, params, [this, event, date, generation, stamp, params, epoch](auto result, bool ok) {
        if (generation != m_calendarSearchGeneration || stamp != calendarSearchContextStamp()) return;
        m_calendarSearchBusy = false; m_calendarSearchStatus.clear();
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
        emit calendarSearchChanged();
        emit calendarSearchResultReady(selected);
    });
}
