// MailBackend's Undo window for calendar deletions and declines: the event is
// hidden at once and the write is sent when the interval ends. Also decides which
// events are visible given pending deletions and the "Show declined" preference.
#include "MailBackend.h"
#include "CalendarLayout.h"
#include <QDateTime>
#include <QJsonArray>

namespace {
// A row names its place in a recurring series in one of three shapes; reduce
// them to one selector with `series_id` and a typed `original_start`.
QVariantMap selector(const QVariantMap &event) {
    for (const auto field : {"recurring_selector", "occurrence_selector"})
        if (!event.value(field).toMap().isEmpty()) return event.value(field).toMap();
    auto key = event.value("occurrence_key").toMap();
    if (!key.isEmpty()) {
        key["series_id"] = key.value("series");
        const auto start = key.value("original_start").toString();
        if (!start.isEmpty()) key["original_start"] = event.value("all_day").toBool()
            ? QVariantMap{{"kind", "date"}, {"date", start}}
            : QVariantMap{{"kind", "zoned"}, {"instant", start}};
    }
    return key;
}
// The row without the display fields calendarPresentation() adds, so two reads
// of the same provider event compare equal whichever day segment they came from.
QVariantMap sourceIdentity(QVariantMap event) {
    for (const auto field : {"column", "columns", "lane", "date", "day_index", "start_hour", "duration_hours", "span_days", "time_display", "multi_day", "start_display", "end_display", "duration_minutes"}) event.remove(field);
    return event;
}
// Whether a queued delete or decline covers `event`: a single event by id, or
// for a recurring scope the exact occurrence, it and every later one
// ("following"), or the whole series.
bool matchesDeletion(const QVariantMap &request, const QVariantMap &event) {
    if (request.value("account") != event.value("account") || request.value("calendar_id") != event.value("calendar_id")) return false;
    auto recurring = request.value("recurring").toMap();
    if (recurring.isEmpty()) recurring = request.value("occurrence").toMap();
    if (recurring.isEmpty()) return request.value("event_id") == event.value("event_id");
    const auto selected = selector(event);
    if (recurring.value("series_id").toString().isEmpty() || recurring.value("series_id") != selected.value("series_id")) return false;
    const auto scope = recurring.value("scope", "occurrence").toString();
    if (scope == "series") return true;
    const auto wanted = recurring.value("original_start").toMap(), actual = selected.value("original_start").toMap();
    if (wanted.value("kind") != actual.value("kind")) return false;
    if (wanted.value("kind") == "date") {
        const auto a = QDate::fromString(actual.value("date").toString(), Qt::ISODate), b = QDate::fromString(wanted.value("date").toString(), Qt::ISODate);
        return a.isValid() && b.isValid() && (scope == "following" ? a >= b : a == b);
    }
    if (wanted.value("kind") != "zoned") return false;
    const auto a = QDateTime::fromString(actual.value("instant").toString(), Qt::ISODate), b = QDateTime::fromString(wanted.value("instant").toString(), Qt::ISODate);
    return a.isValid() && b.isValid() && (scope == "following" ? a >= b : a == b);
}
}

QVariantMap MailBackend::currentCalendarSource(const QVariantMap &event) const {
    for (const auto &value : m_events + m_todayEvents + m_calendarSearchResult.value("events").toList()) {
        const auto row = value.toMap();
        if (row.value("id") == event.value("id") && row.value("account") == event.value("account") && row.value("calendar_id") == event.value("calendar_id")) return row;
    }
    return {};
}
bool MailBackend::calendarEventHidden(const QVariantMap &event) const {
    for (const auto &pending : m_calendarDeletions) if (!pending.decline() && matchesDeletion(pending.request, event)) return true;
    return false;
}
bool MailBackend::calendarEventDeclining(const QVariantMap &event) const {
    for (const auto &pending : m_calendarDeletions) if (pending.decline() && matchesDeletion(pending.request, event)) return true;
    return false;
}
QVariantList MailBackend::visibleCalendarEvents(const QVariantList &rows, bool reflow, bool filterByAccount, DeclinedRows declined) const {
    const bool currentOnly = filterByAccount && m_preferences.value("calendar_account_visibility").toString() == "current";
    const bool hideDeclined = declined == DeclinedRows::Hide || (declined == DeclinedRows::Preference && !m_preferences.value("calendar_show_declined").toBool());
    if (m_calendarDeletions.isEmpty() && !currentOnly && !hideDeclined) return rows;
    QVariantList result; bool changed = false;
    for (const auto &value : rows) {
        auto row = value.toMap();
        if ((currentOnly && row.value("account").toString() != m_account) || calendarEventHidden(row)) { changed = true; continue; }
        // A decline waiting out its Undo shows as declined already.
        if (calendarEventDeclining(row)) { row["self_response"] = "declined"; changed = true; }
        if (hideDeclined && row.value("self_response").toString() == "declined") { changed = true; continue; }
        result.append(row);
    }
    if (!changed) return rows;
    return reflow && result.size() != rows.size() ? reflowCalendarPresentation(result) : result;
}
void MailBackend::calendarDeletionVisibilityChanged() { emit eventsChanged(); emit timelineChanged(); emit calendarSearchChanged(); emit remindersChanged(); }

QString MailBackend::queueCalendarDeletion(const QVariantMap &values, const QVariantMap &event) {
    return queueCalendarHold(values, event, false);
}
// Declining an invitation takes it off the calendars at once; the RSVP is
// sent after the Undo interval, so Undo leaves the earlier answer untouched.
QString MailBackend::queueCalendarDecline(const QVariantMap &values, const QVariantMap &event) {
    return queueCalendarHold(values, event, true);
}
QString MailBackend::queueCalendarHold(const QVariantMap &values, const QVariantMap &event, bool decline) {
    const auto account = values.value("account").toString(), calendar = values.value("calendar_id").toString();
    const auto authority = calendarOccurrenceContext(account, calendar), source = currentCalendarSource(event);
    const QString verb = decline ? "declining" : "deleting";
    const bool allowed = decline ? values.value("kind") == "rsvp" && values.value("response") == "declined" : values.value("kind") == "delete" && event.value("can_delete") == true;
    if (authority.isEmpty() || source.isEmpty() || !allowed
        || event.value("account") != account || event.value("calendar_id") != calendar || event.value("id").toString().isEmpty()
        || event.value("event_id").toString().isEmpty() || values.value("event_id") != event.value("event_id")
        || event.value("etag").toString().isEmpty() || values.value("expected_etag") != event.value("etag")
        || values.value("request_id").toString().isEmpty()) return "The event or its calendar changed. Reopen it before " + verb + ".";
    const QStringList fields = decline ? QStringList{"account", "calendar_id", "event_id", "expected_etag", "kind", "request_id", "response", "recurring"}
                                       : QStringList{"account", "calendar_id", "event_id", "expected_etag", "kind", "request_id", "recurring", "occurrence"};
    for (auto it = values.cbegin(); it != values.cend(); ++it)
        if (!fields.contains(it.key())) return QString(decline ? "This response" : "This deletion") + " contains an unexpected field. Reopen the event.";
    const bool recurring = !values.value("recurring").toMap().isEmpty() || !values.value("occurrence").toMap().isEmpty();
    if (recurring) {
        const auto context = event.value("occurrence_ui_context").toMap();
        const auto captured = QJsonDocument::fromJson(context.value("source").toString().toUtf8()).object();
        if (captured != QJsonObject::fromVariantMap(source)
            || QJsonDocument::fromJson(context.value("authority").toString().toUtf8()).object() != QJsonObject::fromVariantMap(authority)) return "Check this recurring scope again before " + verb + ".";
        auto expected = event.value("recurring_context").toMap();
        if (!decline && expected.value("scope") == "following") expected["reset_future_exceptions"] = true;
        const auto occurrence = decline || !expected.isEmpty() ? QVariantMap{} : event.value("occurrence").toMap();
        if (values.value("recurring").toMap() != expected || values.value("occurrence").toMap() != occurrence)
            return QString(decline ? "The recurring response" : "The recurring deletion") + " no longer matches the checked scope.";
    } else if (source.value("etag") != event.value("etag") || source.value(decline ? "can_rsvp" : "can_delete") != true || source.value("can_resolve_occurrence").toBool() || !source.value("recurring_selector").toMap().isEmpty()) return "The event changed. Reopen it before " + verb + ".";
    if (!matchesDeletion(values, source)) return QString(decline ? "This response" : "This deletion") + " does not match the selected event.";
    for (const auto &pending : m_calendarDeletions)
        if (matchesDeletion(pending.request, source) || matchesDeletion(values, pending.source)) return "A change is already pending for this event or series.";
    if (m_calendarDeletions.size() >= 8) return "Wait for a pending change before changing another event.";
    const auto key = account + '\n' + values.value("request_id").toString();
    CalendarDeletion pending; pending.request = values; pending.source = source; pending.authority = authority; pending.deadline = m_actionClock.elapsed() + m_calendarUndoMs;
    m_calendarDeletions.insert(key, pending);
    const auto scope = values.value("recurring").toMap().value("scope", values.contains("occurrence") ? "occurrence" : "").toString();
    const auto text = decline ? "Declined “"
        : scope == "series" ? "Deleting series “" : scope == "following" ? "Deleting following events for “" : scope == "occurrence" ? "Removing this occurrence of “" : "Deleting “";
    showActionToast(text + source.value("title", "event").toString() + "”", "undo_calendar_delete", {{"key", key}}, m_calendarUndoMs);
    calendarDeletionVisibilityChanged();
    return {};
}
void MailBackend::undoCalendarDeletion(const QString &key) {
    if (!m_calendarDeletions.contains(key) || m_calendarDeletions[key].dispatched || m_actionClock.elapsed() >= m_calendarDeletions[key].deadline) return;
    const bool decline = m_calendarDeletions[key].decline();
    m_calendarDeletions.remove(key); calendarDeletionVisibilityChanged();
    showActionToast(decline ? "Decline undone" : "Deletion undone", {}, {}, 2800, true);
}
void MailBackend::cancelQueuedCalendarDeletions() {
    bool changed = false;
    for (const auto &key : m_calendarDeletions.keys()) if (!m_calendarDeletions[key].dispatched) {
        m_calendarDeletions.remove(key); changed = true;
        if (m_actionToast.value("action") == "undo_calendar_delete" && m_actionToast.value("key") == key) { m_actionToast = {}; emit actionToastChanged(); }
    }
    if (changed) calendarDeletionVisibilityChanged();
}
void MailBackend::advanceCalendarDeletions() {
    for (const auto &key : m_calendarDeletions.keys()) {
        const auto pending = m_calendarDeletions.value(key);
        if (pending.confirmed) {
            // Google accepted the decline; drop the hold once the refreshed
            // event (or its absence) shows it, or after a minute.
            const auto current = currentCalendarSource(pending.source);
            if (current.isEmpty() || sourceIdentity(current) != sourceIdentity(pending.source) || m_actionClock.elapsed() >= pending.deadline) {
                m_calendarDeletions.remove(key); calendarDeletionVisibilityChanged();
            }
            continue;
        }
        if (pending.dispatched) continue;
        // The write was prepared against a specific etag and calendar
        // permission. If either moved during the Undo window, cancel rather
        // than send a stale write.
        const auto current = currentCalendarSource(pending.source);
        if (pending.authority != calendarOccurrenceContext(pending.request.value("account").toString(), pending.request.value("calendar_id").toString())
            || (!current.isEmpty() && sourceIdentity(current) != sourceIdentity(pending.source))) {
            m_calendarDeletions.remove(key); calendarDeletionVisibilityChanged();
            if (m_actionToast.value("key") == key) { m_actionToast = {}; emit actionToastChanged(); }
            showActionToast(pending.decline() ? "Decline cancelled because the event or calendar changed." : "Deletion cancelled because the event or calendar changed.", {}, {}, 2800, true);
        } else if (m_actionClock.elapsed() >= pending.deadline) {
            m_calendarDeletions[key].dispatched = true;
            calendarWrite(pending.request);
        }
    }
}
void MailBackend::finishQueuedCalendarDeletion(const QString &account, const QString &id, bool confirmed, const QString &message) {
    const auto key = account + '\n' + id;
    if (!m_calendarDeletions.contains(key)) return;
    if (confirmed && m_calendarDeletions[key].decline()) {
        auto &pending = m_calendarDeletions[key];
        pending.confirmed = true; pending.deadline = m_actionClock.elapsed() + 60000;
        m_actionTimer.start();
        return;
    }
    const auto pending = m_calendarDeletions.take(key);
    if (confirmed) showActionToast("Deleted “" + pending.source.value("title", "event").toString() + "”", {}, {}, 2800, true);
    else {
        calendarDeletionVisibilityChanged();
        showActionToast((pending.decline() ? "Decline needs attention. " : "Deletion needs attention. ") + message, "calendar_activity", {}, 6000, true);
    }
}
