#include "MailBackend.h"
#include "CalendarLayout.h"
#include <QDateTime>
#include <QJsonArray>

namespace {
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
QVariantMap sourceIdentity(QVariantMap event) {
    for (const auto field : {"column", "columns", "lane", "date", "day_index", "start_hour", "duration_hours", "span_days", "time_display", "multi_day", "start_display", "end_display", "duration_minutes"}) event.remove(field);
    return event;
}
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
    for (const auto &pending : m_calendarDeletions) if (matchesDeletion(pending.request, event)) return true;
    return false;
}
QVariantList MailBackend::visibleCalendarEvents(const QVariantList &rows, bool reflow) const {
    if (m_calendarDeletions.isEmpty()) return rows;
    QVariantList result;
    for (const auto &row : rows) if (!calendarEventHidden(row.toMap())) result.append(row);
    return reflow && result.size() != rows.size() ? reflowCalendarPresentation(result) : result;
}
void MailBackend::calendarDeletionVisibilityChanged() { emit eventsChanged(); emit calendarSearchChanged(); emit remindersChanged(); }

QString MailBackend::queueCalendarDeletion(const QVariantMap &values, const QVariantMap &event) {
    const auto account = values.value("account").toString(), calendar = values.value("calendar_id").toString();
    const auto authority = calendarOccurrenceContext(account, calendar), source = currentCalendarSource(event);
    if (authority.isEmpty() || source.isEmpty() || values.value("kind") != "delete" || event.value("can_delete") != true
        || event.value("account") != account || event.value("calendar_id") != calendar || event.value("id").toString().isEmpty()
        || event.value("event_id").toString().isEmpty() || values.value("event_id") != event.value("event_id")
        || event.value("etag").toString().isEmpty() || values.value("expected_etag") != event.value("etag")
        || values.value("request_id").toString().isEmpty()) return "The event or its calendar changed. Reopen it before deleting.";
    for (auto it = values.cbegin(); it != values.cend(); ++it)
        if (!QStringList{"account", "calendar_id", "event_id", "expected_etag", "kind", "request_id", "recurring", "occurrence"}.contains(it.key())) return "This deletion contains an unexpected field. Reopen the event.";
    const bool recurring = !values.value("recurring").toMap().isEmpty() || !values.value("occurrence").toMap().isEmpty();
    if (recurring) {
        const auto context = event.value("occurrence_ui_context").toMap();
        const auto captured = QJsonDocument::fromJson(context.value("source").toString().toUtf8()).object();
        if (captured != QJsonObject::fromVariantMap(source)
            || QJsonDocument::fromJson(context.value("authority").toString().toUtf8()).object() != QJsonObject::fromVariantMap(authority)) return "Check this recurring scope again before deleting.";
        auto expected = event.value("recurring_context").toMap();
        if (expected.value("scope") == "following") expected["reset_future_exceptions"] = true;
        if (values.value("recurring").toMap() != expected || values.value("occurrence").toMap() != (expected.isEmpty() ? event.value("occurrence").toMap() : QVariantMap{})) return "The recurring deletion no longer matches the checked scope.";
    } else if (source.value("etag") != event.value("etag") || source.value("can_delete") != true || source.value("can_resolve_occurrence").toBool() || !source.value("recurring_selector").toMap().isEmpty()) return "The event changed. Reopen it before deleting.";
    if (!matchesDeletion(values, source)) return "This deletion does not match the selected event.";
    for (const auto &pending : m_calendarDeletions)
        if (matchesDeletion(pending.request, source) || matchesDeletion(values, pending.source)) return "A deletion is already pending for this event or series.";
    if (m_calendarDeletions.size() >= 8) return "Wait for a pending deletion before deleting another event.";
    const auto key = account + '\n' + values.value("request_id").toString();
    CalendarDeletion pending; pending.request = values; pending.source = source; pending.authority = authority; pending.deadline = m_actionClock.elapsed() + m_calendarUndoMs;
    m_calendarDeletions.insert(key, pending);
    const auto scope = values.value("recurring").toMap().value("scope", values.contains("occurrence") ? "occurrence" : "").toString();
    const auto text = scope == "series" ? "Deleting series “" : scope == "following" ? "Deleting following events for “" : scope == "occurrence" ? "Removing this occurrence of “" : "Deleting “";
    showActionToast(text + source.value("title", "event").toString() + "”", "undo_calendar_delete", {{"key", key}}, m_calendarUndoMs);
    calendarDeletionVisibilityChanged();
    return {};
}
void MailBackend::undoCalendarDeletion(const QString &key) {
    if (!m_calendarDeletions.contains(key) || m_calendarDeletions[key].dispatched || m_actionClock.elapsed() >= m_calendarDeletions[key].deadline) return;
    m_calendarDeletions.remove(key); calendarDeletionVisibilityChanged();
    showActionToast("Deletion undone", {}, {}, 2800, true);
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
        const auto pending = m_calendarDeletions.value(key); if (pending.dispatched) continue;
        const auto current = currentCalendarSource(pending.source);
        if (pending.authority != calendarOccurrenceContext(pending.request.value("account").toString(), pending.request.value("calendar_id").toString())
            || (!current.isEmpty() && sourceIdentity(current) != sourceIdentity(pending.source))) {
            m_calendarDeletions.remove(key); calendarDeletionVisibilityChanged();
            if (m_actionToast.value("key") == key) { m_actionToast = {}; emit actionToastChanged(); }
            showActionToast("Deletion cancelled because the event or calendar changed.", {}, {}, 2800, true);
        } else if (m_actionClock.elapsed() >= pending.deadline) {
            m_calendarDeletions[key].dispatched = true;
            calendarWrite(pending.request);
        }
    }
}
void MailBackend::finishQueuedCalendarDeletion(const QString &account, const QString &id, bool confirmed, const QString &message) {
    const auto key = account + '\n' + id;
    if (!m_calendarDeletions.contains(key)) return;
    const auto pending = m_calendarDeletions.take(key);
    if (confirmed) showActionToast("Deleted “" + pending.source.value("title", "event").toString() + "”", {}, {}, 2800, true);
    else {
        calendarDeletionVisibilityChanged();
        showActionToast("Deletion needs attention. " + message, "calendar_activity", {}, 6000, true);
    }
}
