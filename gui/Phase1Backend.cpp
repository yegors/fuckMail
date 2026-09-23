#include "MailBackend.h"
#include "CalendarLayout.h"
#include <QDate>
#include <QJsonArray>
#include <QTimeZone>
#include <QJsonDocument>
#include <QDesktopServices>
#include <QApplication>
#include <QThreadPool>
#include <cmath>
#include <algorithm>
#ifdef Q_OS_WIN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <mmsystem.h>
#endif

namespace {
QVariantMap calendarPolicies(const QVariantList &sources) {
    QVariantMap policies;
    for (const auto &value : sources) {
        const auto source = value.toMap();
        const auto key = QString::fromUtf8(QJsonDocument(QJsonArray{source.value("account").toString(), source.value("calendar_id").toString()}).toJson(QJsonDocument::Compact));
        policies[key] = QVariantMap{{"permission", source.value("permission")}, {"writable", source.value("writable")},
            {"can_create", source.value("can_create")}, {"time_zone", source.value("time_zone")}};
    }
    return policies;
}
QString reminderKey(const QVariantMap &event) {
    return QString::fromUtf8(QJsonDocument(QJsonArray{event.value("account").toString(), event.value("calendar_id").toString(),
        event.value("id").toString(), event.value("start").toString()}).toJson(QJsonDocument::Compact));
}
bool changesCalendarQuery(const QVariantMap &before, const QVariantMap &after) {
    for (const auto &key : {"week_start", "selected_calendars", "calendar_account_visibility", "calendar_view", "calendar_date", "timeline_date", "calendar_display_time_zone"})
        if (before.value(key) != after.value(key)) return true;
    return before.isEmpty();
}
bool reminderReceiptMatches(const QJsonObject &receipt, const QVariantMap &event, const QString &state) {
    const auto current = receipt["event"].toObject().toVariantMap();
    return receipt["account"].toString() == event.value("account").toString()
        && receipt["calendar_id"].toString() == event.value("calendar_id").toString()
        && receipt["occurrence_id"].toString() == event.value("id").toString()
        && receipt["start"].toString() == event.value("start").toString()
        && receipt["state"].toString() == state && reminderKey(current) == reminderKey(event)
        && !current.value("busy_only").toBool() && !current.value("cancelled").toBool()
        && current.value("self_response").toString() != "declined";
}
QVariantList notificationAccounts(const QVariantList &accounts) {
    QVariantList result;
    for (const auto &value : accounts) {
        const auto account = value.toMap(), google = account.value("google").toMap();
        result.append(QVariantMap{{"id", account.value("id")}, {"lifecycle", account.value("lifecycle")},
            {"connection_state", google.value("connection_state")}, {"granted_scopes", google.value("granted_scopes")}});
    }
    return result;
}
}

QString MailBackend::notificationContext() const {
    QVariantList sources;
    for (const auto &value : m_calendarSources) {
        const auto source = value.toMap();
        sources.append(QVariantMap{{"account",source.value("account")}, {"calendar_id",source.value("calendar_id")},
            {"revision",source.value("revision")}, {"permission",source.value("permission")}, {"time_zone",source.value("time_zone")}});
    }
    return QString::fromUtf8(QJsonDocument::fromVariant(QVariantList{notificationAccounts(m_allAccounts), sources,
        m_preferences.value("selected_calendars"), m_preferences.value("calendar_display_time_zone")}).toJson(QJsonDocument::Compact));
}

void MailBackend::configureNotifications() {
    const auto mode = m_preferences.value("notifications", "events").toString();
    const auto enabled = m_connected && startupReady() && !m_shutdown && !m_syntheticProfile && mode != "off";
    const auto mailConfig = enabled && mode == "all" ? QString::fromUtf8(QJsonDocument::fromVariant(
        notificationAccounts(m_allAccounts)).toJson(QJsonDocument::Compact)) : QString{};
    if (mailConfig != m_mailNotificationConfig) {
        // Calendar-only changes do not discard the arrival cursor. Starting a
        // new notification session samples MAX before normal background work.
        if (mailConfig.isEmpty() || m_mailNotificationConfig.isEmpty()) {
            m_mailNotificationCursor = -1; m_notificationsSince = QDateTime::currentDateTimeUtc();
        }
        m_mailNotificationConfig = mailConfig; ++m_mailNotificationGeneration; m_mailNotificationPolling = false;
    }
    const auto config = enabled ? mode + notificationContext() : QString{};
    if (config == m_notificationConfig) return;
    m_notificationConfig = config; ++m_notificationGeneration; m_notificationPolling = false;
    if (!m_reminders.isEmpty()) { m_reminders.clear(); emit remindersChanged(); }
    if (config.isEmpty()) m_notificationTimer.stop();
    else { m_notificationTimer.start(); QTimer::singleShot(0, this, &MailBackend::pollNotifications); }
}

void MailBackend::pollMailNotifications() {
    if (m_mailNotificationConfig.isEmpty() || !m_connected || m_shutdown || m_mailNotificationPolling
        || (m_mailNotificationCursor >= 0 && (m_pending.size() > 8 || (m_queryActivity.isValid() && m_queryActivity.elapsed() < 500)))) return;
    const int generation = m_mailNotificationGeneration;
    QJsonObject params;
    if (m_mailNotificationCursor >= 0) {
        params["after"] = m_mailNotificationCursor;
        params["since"] = m_notificationsSince.toString(Qt::ISODateWithMs);
    }
    m_mailNotificationPolling = true;
    request("mail_notifications", {}, params, [this, generation](auto mail, bool loaded) {
        if (generation != m_mailNotificationGeneration) return;
        m_mailNotificationPolling = false;
        if (!loaded || !m_connected || m_shutdown || !mail["after"].isDouble()
            || mail["after"].toInteger(-1) < qMax<qint64>(0, m_mailNotificationCursor)) return;
        m_mailNotificationCursor = mail["after"].toInteger();
        const auto messages = mail["messages"].toArray();
        if (messages.isEmpty()) return;
        if (mail["total"].toInteger() == 1) {
            const auto message = messages.first().toObject();
            emit desktopNotification(message["sender"].toString(), message["subject"].toString());
        } else emit desktopNotification("New mail", QString::number(mail["total"].toInteger()) + " new messages");
    });
}

void MailBackend::pollNotifications() {
    pollMailNotifications();
    if (m_notificationConfig.isEmpty() || !m_connected || m_shutdown || m_notificationPolling
        || m_pending.size() > 8 || (m_queryActivity.isValid() && m_queryActivity.elapsed() < 500)) return;
    const int generation = m_notificationGeneration;
    m_notificationPolling = true;
    request("calendar_reminders", {}, {}, [this, generation](auto result, bool ok) {
        if (generation != m_notificationGeneration) return;
        m_notificationPolling = false;
        if (!ok || m_shutdown) {
            if (!m_reminders.isEmpty()) { m_reminders.clear(); emit remindersChanged(); }
            return;
        }
        QVariantList retained;
        QList<QVariantMap> fresh;
        QSet<QString> currentKeys;
        for (const auto &value : result["reminders"].toArray()) {
            const auto event = value.toObject().toVariantMap(); const auto key = reminderKey(event);
            currentKeys.insert(key);
            if (m_remindersPresented.contains(key)) retained.append(event);
            else if (!calendarEventHidden(event) && event.value("reminder_state").isNull() && fresh.size() < 8) fresh.append(event);
        }
        m_remindersPresented.intersect(currentKeys);
        if (retained != m_reminders) { m_reminders = retained; emit remindersChanged(); }
        for (const auto &event : fresh) {
            const auto account = event.value("account").toString();
            const auto key = reminderKey(event);
            const QJsonObject params{{"calendar_id",event.value("calendar_id").toString()}, {"occurrence_id",event.value("id").toString()},
                {"start",event.value("start").toString()}, {"state","shown"}};
            request("calendar_reminder_ack", account, params, [this, generation, event, key, account](auto receipt, bool acknowledged) {
                if (generation != m_notificationGeneration || !acknowledged || m_shutdown || accountRemoved(account)
                    || !reminderReceiptMatches(receipt, event, "shown") || m_remindersPresented.contains(key)) return;
                auto current = receipt["event"].toObject().toVariantMap(); current["timing"] = event.value("timing"); current["due_at"] = event.value("due_at");
                m_remindersPresented.insert(key); m_reminders.append(current); emit remindersChanged();
                if (calendarEventHidden(current)) return;
                emit desktopNotification(current.value("timing").toString() + " · " + current.value("title").toString(),
                    current.value("account_name").toString() + " · " + current.value("calendar_name").toString());
            });
        }
    });
}

void MailBackend::dismissCalendarReminder(const QVariantMap &event) { acknowledgeCalendarReminder(event, false); }
void MailBackend::joinCalendarEvent(const QVariantMap &event) { acknowledgeCalendarReminder(event, true); }
void MailBackend::snoozeCalendarReminder(const QVariantMap &event) { acknowledgeCalendarReminder(event, false, true); }
void MailBackend::playReminderSound(int level) {
    if (level < 0 || level > 2 || m_shutdown) return;
#ifdef Q_OS_WIN
    // Build a short PCM waveform in memory. The UI controls repeat cadence;
    // each tone has a soft envelope so stopping one cannot click loudly.
    constexpr int rate = 22050;
    struct Note { double hz; double seconds; };
    const QList<Note> notes = level == 0 ? QList<Note>{{660,0.16},{880,0.22}}
        : level == 1 ? QList<Note>{{880,0.11},{1175,0.11},{1568,0.18}}
        : QList<Note>{{880,0.17},{660,0.17},{880,0.17},{660,0.17}};
    QByteArray pcm;
    for (const auto &note : notes) {
        const int samples = int(rate * note.seconds);
        for (int i=0; i<samples; ++i) {
            const double phase = std::fmod(note.hz * i / rate, 1.0);
            const double wave = level == 0 ? std::sin(phase * 6.283185307179586)
                : level == 1 ? 1.0 - 4.0 * std::abs(phase - 0.5)
                : (phase < 0.5 ? 1.0 : -1.0);
            const double envelope = std::min({1.0, i / (rate * 0.012), (samples-i-1) / (rate * 0.02)});
            const qint16 value = qint16(wave * std::max(0.0,envelope) * (level == 2 ? 4800 : 3000));
            pcm.append(char(value & 0xff)); pcm.append(char((value >> 8) & 0xff));
        }
    }
    QByteArray wav; wav.reserve(44+pcm.size());
    auto u16=[&wav](quint16 v){wav.append(char(v&255));wav.append(char((v>>8)&255));};
    auto u32=[&wav](quint32 v){for(int i=0;i<4;++i)wav.append(char((v>>(8*i))&255));};
    wav.append("RIFF",4);u32(36+pcm.size());wav.append("WAVEfmt ",8);u32(16);u16(1);u16(1);u32(rate);u32(rate*2);u16(2);u16(16);wav.append("data",4);u32(pcm.size());wav.append(pcm);
    QThreadPool::globalInstance()->start([wave=std::move(wav)] {
        PlaySoundA(wave.constData(), nullptr, SND_MEMORY | SND_SYNC | SND_NODEFAULT);
    });
#else
    QApplication::beep();
#endif
}
void MailBackend::stopReminderSound() {
#ifdef Q_OS_WIN
    PlaySoundA(nullptr, nullptr, 0);
#endif
}
QString MailBackend::calendarReminderWhen(const QVariantMap &event) const {
    if (event.value("all_day").toBool()) return QDate::fromString(event.value("start").toString(),Qt::ISODate).toString("dddd d MMM") + " · all day";
    auto start=QDateTime::fromString(event.value("start").toString(),Qt::ISODate);
    auto end=QDateTime::fromString(event.value("end").toString(),Qt::ISODate);
    if (!start.isValid() || !end.isValid()) return {};
    const QTimeZone zone(m_preferences.value("calendar_display_time_zone").toString().toUtf8());
    if (zone.isValid()) { start=start.toTimeZone(zone); end=end.toTimeZone(zone); }
    const auto minutes=std::max<qint64>(0,start.secsTo(end)/60);
    const auto duration=minutes>=60 ? QString::number(minutes/60)+" h"+(minutes%60 ? " "+QString::number(minutes%60)+" min" : "") : QString::number(minutes)+" min";
    return start.toString("dddd d MMM · HH:mm") + " – " + end.toString(start.date()==end.date() ? "HH:mm" : "ddd d MMM HH:mm") + " · " + duration;
}
void MailBackend::acknowledgeCalendarReminder(const QVariantMap &event, bool join, bool snooze) {
    if (!m_connected || m_shutdown) return;
    const auto account = event.value("account").toString();
    if (account.isEmpty() || accountRemoved(account)) return;
    const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    const auto context = notificationContext();
    const QJsonObject params{{"calendar_id",event.value("calendar_id").toString()}, {"occurrence_id",event.value("id").toString()},
        {"start",event.value("start").toString()}, {"state",snooze ? "snoozed" : "dismissed"}};
    request("calendar_reminder_ack", account, params, [this, account, lifecycle, context, event, join, snooze](auto receipt, bool ok) {
        if (!ok || m_shutdown || accountRemoved(account)
            || context != notificationContext() || !reminderReceiptMatches(receipt, event, snooze ? "snoozed" : "dismissed")
            || lifecycle != lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong()) return;
        const auto key = reminderKey(event);
        m_reminders.removeIf([&key](const auto &row) { return reminderKey(row.toMap()) == key; }); emit remindersChanged();
        if (snooze) m_remindersPresented.remove(key);
        if (join) {
            const QUrl url(receipt["event"].toObject()["conference_url"].toString());
            if (url.isValid() && url.scheme() == "https" && !url.host().isEmpty() && url.userInfo().isEmpty())
                QDesktopServices::openUrl(url);
        }
    });
}

QDateTime MailBackend::clockNow() const {
    const QTimeZone chosen(m_preferences.value("calendar_display_time_zone").toString().toUtf8());
    const auto zone = m_syntheticProfile ? QTimeZone("America/Toronto") : chosen.isValid() ? chosen : QTimeZone::systemTimeZone();
    return m_fixtureClock ? QDateTime::fromString("2026-09-11T10:42:00-04:00", Qt::ISODate).toTimeZone(zone)
                          : QDateTime::currentDateTimeUtc().toTimeZone(zone);
}
QString MailBackend::todayDate() const { return clockNow().date().toString(Qt::ISODate); }
QString MailBackend::calendarDateTime(const QString &date, const QString &time, const QString &zone) const {
    const auto day = QDate::fromString(date, Qt::ISODate);
    const auto clock = QTime::fromString(time, "HH:mm");
    const QTimeZone tz(zone.toUtf8());
    if (!day.isValid() || !clock.isValid() || !tz.isValid()) return {};
    const QDateTime value(day, clock, tz);
    if (!value.isValid() || value.date() != day || value.time() != clock) return {};
    return value.toString(Qt::ISODate);
}
double MailBackend::nowHour() const { const auto t = clockNow().time(); return t.hour() + t.minute() / 60.0; }
QVariantList MailBackend::calendarPreview(const QVariantMap &event) const {
    const QTimeZone zone(m_preferences.value("calendar_display_time_zone").toString().toUtf8());
    return calendarPresentation({QJsonObject::fromVariantMap(event)}, QDate::fromString(calendarFrom(), Qt::ISODate), calendarDays(), zone);
}
int MailBackend::calendarDays() const { return m_calendarView == "day" ? 1 : m_calendarView == "week" ? 7 : 42; }
QString MailBackend::calendarFrom() const {
    auto day = QDate::fromString(m_calendarDate, Qt::ISODate);
    if (m_calendarView == "day" || m_calendarView == "agenda") return day.toString(Qt::ISODate);
    if (m_calendarView == "month") day = QDate(day.year(), day.month(), 1);
    const int first = m_preferences.value("week_start", "monday").toString() == "sunday" ? 7 : 1;
    return day.addDays(-((day.dayOfWeek() - first + 7) % 7)).toString(Qt::ISODate);
}
void MailBackend::setCalendarView(const QString &view) {
    if (view != "day" && view != "week" && view != "month" && view != "agenda") return;
    if (view == m_calendarView) return;
    m_calendarView = view; setPreferences({{"calendar_view", view}}); loadCalendar(); emit eventsChanged();
}
void MailBackend::setCalendarDate(const QString &date) {
    if (!QDate::fromString(date, Qt::ISODate).isValid() || date == m_calendarDate) return;
    m_calendarDate = date; setPreferences({{"calendar_date", date}}); loadCalendar(); emit eventsChanged();
}
void MailBackend::moveCalendar(int delta) {
    auto day = QDate::fromString(m_calendarDate, Qt::ISODate);
    day = m_calendarView == "month" ? day.addMonths(delta) : day.addDays(delta * (m_calendarView == "week" ? 7 : m_calendarView == "agenda" ? 42 : 1));
    setCalendarDate(day.toString(Qt::ISODate));
}
void MailBackend::showCalendarToday() { setCalendarDate(todayDate()); }
void MailBackend::moveTimeline(int delta) {
    m_timelineDate = QDate::fromString(m_timelineDate, Qt::ISODate).addDays(delta).toString(Qt::ISODate);
    setPreferences({{"timeline_date", m_timelineDate}}); loadCalendar(); emit eventsChanged();
}
void MailBackend::showTimelineToday() { m_timelineDate = todayDate(); setPreferences({{"timeline_date", m_timelineDate}}); loadCalendar(); emit eventsChanged(); }
void MailBackend::loadCalendarSources() {
    request("calendar_sources", {}, {}, [this](auto result, bool ok) {
        if (!ok) return;
        const auto sources = result["calendars"].toArray().toVariantList();
        if (m_calendarSources == sources) return;
        const auto previousSelection = effectiveCalendarSelection();
        // A successful permission recheck advances the store's general source
        // revision. Only an observed authority change should invalidate the
        // very occurrence lookup that performed that recheck. Keep a local
        // epoch so a revoke/restore cycle still fences older UI callbacks.
        if (calendarPolicies(m_calendarSources) != calendarPolicies(sources)) ++m_calendarAuthorityEpoch;
        m_calendarSources = sources; emit eventsChanged();
        if (previousSelection != effectiveCalendarSelection()) loadCalendar();
    });
}
void MailBackend::calendarWrite(const QVariantMap &values) {
    auto params = QJsonObject::fromVariantMap(values);
    const auto account = params.take("account").toString();
    if (!params.contains("request_id")) params["request_id"] = mutation()["request_id"];
    const auto requestId = params["request_id"].toString();
    ++m_calendarWrites;
    if (account.isEmpty() || !m_connected || m_shutdown) {
        finishCalendarOperation(account, requestId, {{"code", "calendar_unavailable"}, {"local_preflight", true}, {"operation_action", "write"}, {"message", "Connect to the app and choose a writable calendar account."}}, false); return;
    }
    if (!m_syntheticProfile && (!accountCapability(account, "calendar_write") || !googleSyncState(account).value("active").toBool())) {
        finishCalendarOperation(account, requestId, {{"code", "calendar_sync_required"}, {"local_preflight", true}, {"operation_action", "write"}, {"message", "Start Calendar sync for this Google account before saving. Your fields remain here."}}, false); return;
    }
    const auto completed = [this, account, requestId](auto result, bool ok) {
        result["operation_action"] = "write";
        finishCalendarOperation(account, requestId, result, ok);
    };
    if (m_syntheticProfile) request("calendar_write", account, params, completed);
    else googleOperation("google_calendar_write", account, params, completed);
}

void MailBackend::loadPreferences() {
    request("preferences_get", {}, {}, [this](auto result, bool ok) {
        if (!ok || m_preferencesSaving) return;
        if (!m_initialPreferencesLoaded) { m_initialPreferencesLoaded = true; emit startupReadyChanged(); }
        m_preferencesRevision = result["revision"].toInteger();
        auto preferences = result["preferences"].toObject().toVariantMap();
        for (auto it = m_preferencesPending.cbegin(); it != m_preferencesPending.cend(); ++it) preferences[it.key()] = it.value();
        if (preferences != m_preferences) {
            const bool reloadCalendar = changesCalendarQuery(m_preferences, preferences);
            m_preferences = preferences;
            m_calendarView = preferences.value("calendar_view", "week").toString();
            m_calendarDate = preferences.value("calendar_date").isNull() ? todayDate() : preferences.value("calendar_date").toString();
            m_timelineDate = preferences.value("timeline_date").isNull() ? todayDate() : preferences.value("timeline_date").toString();
            emit preferencesChanged(); if (reloadCalendar) loadCalendar();
        }
        resumeStartupGoogleSync();
        if (!m_syntheticProfile && m_preferences.value("calendar_display_time_zone").isNull()
            && !m_preferencesPending.contains("calendar_display_time_zone")) {
            // Resolve the host zone once; the shared Core validates the IANA ID.
            const auto systemZone = QTimeZone::systemTimeZone();
            if (systemZone.isValid()) setPreferences({{"calendar_display_time_zone", QString::fromUtf8(systemZone.id())}});
        }
    });
}
void MailBackend::setPreferences(const QVariantMap &patch) {
    if (patch.isEmpty() && m_preferencesPending.isEmpty()) return;
    const auto previousVisibility = m_preferences.value("calendar_account_visibility");
    for (auto it = patch.cbegin(); it != patch.cend(); ++it) { m_preferences[it.key()] = it.value(); m_preferencesPending[it.key()] = it.value(); }
    if (previousVisibility != m_preferences.value("calendar_account_visibility")) emit eventsChanged();
    m_preferencesStatus = "Saving…"; emit preferencesChanged(); savePreferences();
}
void MailBackend::savePreferences() {
    if (m_preferencesSaving || m_preferencesPending.isEmpty()) return;
    if (!m_connected) { m_preferencesStatus = "Not saved — reconnect and retry."; cancelShutdown(); emit preferencesChanged(); return; }
    const auto submitted = m_preferencesPending;
    auto params = mutation(); params["preferences"] = QJsonObject::fromVariantMap(submitted); params["expected_revision"] = m_preferencesRevision;
    m_preferencesSaving = true;
    request("preferences_save", {}, params, [this, submitted](auto result, bool ok) {
        m_preferencesSaving = false;
        if (!ok) {
            m_preferencesStatus = "Not saved — " + result["message"].toString("retry after reconnecting");
            cancelShutdown(); emit preferencesChanged();
            // Refresh the optimistic base while keeping the user's pending edits.
            // Retrying then merges them with the other client's committed values.
            if (result["code"].toString() == "stale_revision") loadPreferences();
            return;
        }
        m_preferencesRevision = result["revision"].toInteger();
        for (auto it = submitted.cbegin(); it != submitted.cend(); ++it)
            if (m_preferencesPending.value(it.key()) == it.value()) m_preferencesPending.remove(it.key());
        m_preferencesStatus = m_preferencesPending.isEmpty() ? "Saved on this machine" : "Saving…";
        emit preferencesChanged();
        for (const auto &key : {"week_start", "selected_calendars", "calendar_account_visibility", "calendar_display_time_zone"})
            if (submitted.contains(key)) { loadCalendar(); break; }
        if (!m_preferencesPending.isEmpty()) savePreferences();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::updateAccountPreferences(const QString &account, const QString &name, const QString &signature, const QString &retention, const QVariantMap &sync) {
    qint64 revision = -1; bool retentionChanged = false;
    for (const auto &row : m_accounts) if (row.toMap()["id"].toString() == account) {
        revision = row.toMap()["metadata_revision"].toLongLong();
        retentionChanged = !retention.isEmpty() && retention != row.toMap()["mail_retention"].toString();
    }
    if (revision < 0) return;
    auto params = mutation(); params["expected_revision"] = revision; params["name"] = name; params["signature_text"] = signature;
    if (!retention.isEmpty()) params["mail_retention"] = retention;
    if (!sync.isEmpty()) params["sync"] = QJsonObject::fromVariantMap(sync);
    m_preferencesStatus = "Saving…"; emit preferencesChanged();
    ++m_accountWrites;
    request("account_update", account, params, [this, account, retentionChanged](auto result, bool ok) {
        --m_accountWrites;
        if (!ok) cancelShutdown();
        m_preferencesStatus = ok ? "Saved on this machine — existing replies keep their saved signature" : "Not saved — " + result["message"].toString();
        emit preferencesChanged();
        if (ok || result["code"].toString() == "stale_revision") loadAccounts();
        if (ok) {
            loadCalendarSources(); loadPreferences();
            m_mailViewCache.clear(); m_readerCache.clear(); m_sourceCache.clear(); m_bodyCache.clear();
            if (retentionChanged && account == m_account) {
                ++m_generation; ++m_readerLoadSerial; m_readerLoadingGeneration = -1;
                m_readerReviewPending = false; m_readerFuture.cancel(); m_readerPrefetchFuture.cancel();
                m_pinnedRow.clear(); m_pinnedIndex = -1; m_conversation.clear();
                emit conversationChanged(); loadConversation(false, false);
            }
            loadConversations(false, true);
        }
        emit accountPreferencesFinished(account, ok);
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::setCalendarSelected(const QString &account, const QString &calendar, bool selected) {
    QVariantList values;
    const auto current = m_preferences.value("selected_calendars");
    if (current.isNull() || !current.isValid()) {
        for (const auto &source : m_calendarSources) { const auto row = source.toMap(); values.append(QVariantMap{{"account", row["account"]}, {"calendar_id", row["calendar_id"]}}); }
    } else values = current.toList();
    QVariantList next; bool found = false;
    for (const auto &source : values) {
        const auto row = source.toMap(); const bool match = row["account"].toString() == account && row["calendar_id"].toString() == calendar;
        if (match) found = true;
        if (!match || selected) next.append(row);
    }
    if (selected && !found) next.append(QVariantMap{{"account", account}, {"calendar_id", calendar}});
    setPreferences({{"selected_calendars", next}});
}

QJsonValue MailBackend::effectiveCalendarSelection() const {
    auto selection = QJsonValue::fromVariant(m_preferences.value("selected_calendars"));
    if (selection.isUndefined()) selection = QJsonValue::Null;
    if (m_preferences.value("calendar_account_visibility").toString() != "current") return selection;
    const auto candidates = selection.isArray() ? selection.toArray() : QJsonArray::fromVariantList(m_calendarSources);
    QJsonArray selected;
    for (const auto &value : candidates) {
        const auto source = value.toObject();
        if (source["account"].toString() == m_account)
            selected.append(QJsonObject{{"account",m_account},{"calendar_id",source["calendar_id"]}});
    }
    return selected;
}

bool MailBackend::validFilterSender(const QString &sender) const {
    if (sender.toUtf8().size() > 256) return false;
    for (const auto character : sender)
        if (character.category() == QChar::Other_Control) return false;
    return true;
}
void MailBackend::applyFilters(const QVariantMap &filters) {
    if (filters == m_filters) return;
    m_queryActivity.start();
    m_filters = filters; m_pinnedRow.clear(); m_pinnedIndex = -1; remember("filters", filters); ++m_listGeneration; emit queryChanged(); loadConversations();
}
void MailBackend::openConversation(const QString &id) { selectConversationInternal(id, true); }
void MailBackend::clearSenderCard() {
    ++m_senderCardGeneration;
    if (m_senderCard.isEmpty()) return;
    m_senderCard.clear(); emit senderCardChanged();
}
void MailBackend::requestSenderCard(const QString &email) {
    const auto address = email.trimmed().toLower();
    if (!m_connected || m_account.isEmpty() || address.isEmpty() || !validFilterSender(address)) {
        clearSenderCard(); return;
    }
    const auto account = m_account;
    const int generation = ++m_senderCardGeneration;
    m_senderCard = {{"account", account}, {"email", address}, {"loading", true}, {"conversations", QVariantList{}}};
    emit senderCardChanged();
    request("sender_summary", account, {{"email", address}}, [this, account, address, generation](auto result, bool ok) {
        if (account != m_account || generation != m_senderCardGeneration) return;
        m_senderCard = ok ? result.toVariantMap() : QVariantMap{{"account", account}, {"email", address}, {"error", result["message"].toString("Could not load this sender.")}};
        m_senderCard["loading"] = false; emit senderCardChanged();
    });
}
void MailBackend::doneConversation() { mutateConversation("flag", {{"read", true}, {"starred", false}}); }
void MailBackend::editFormattedDraft(const QString &body, const QString &html) {
    if (draftLocked() || m_draft.isEmpty() || (m_draft["body"].toString() == body && m_draft["body_html"].toString() == html)) return;
    m_draft["body"] = body; m_draft["body_html"] = html;
    m_draftDirty = true; m_draftStatus = "Saving…"; emit draftChanged(); m_draftTimer.start();
}
void MailBackend::setDraftAttachment(const QString &id, bool selected) {
    if (draftLocked() || m_draft.isEmpty()) return;
    auto ids = m_draft["attachment_ids"].toList();
    if (selected && !ids.contains(id)) ids.append(id);
    if (!selected) ids.removeAll(id);
    if (ids == m_draft["attachment_ids"].toList()) return;
    m_draft["attachment_ids"] = ids; m_draftDirty = true; m_draftStatus = "Saving…";
    emit draftChanged(); m_draftTimer.start();
}
void MailBackend::setDraftSignature(const QString &signature) {
    if (draftLocked() || m_draft.isEmpty() || m_draft["signature_text"].toString() == signature) return;
    m_draft["signature_text"] = signature; m_draftDirty = true; m_draftStatus = "Saving…";
    emit draftChanged(); m_draftTimer.start();
}
void MailBackend::reloadDraft() {
    if (m_unsavedAccountBuffer) { accountNavigationAllowed(); return; }
    cancelPendingGoogleSend("Send cancelled before reloading the draft.");
    if (draftBusy() || attachmentImportPending() || m_draft.isEmpty()) return;
    m_draftTimer.stop(); m_resolvingDraft = true;
    m_draftStatus = "Loading the saved draft…"; emit draftChanged();
    const auto account = m_account, id = draftContext();
    request("draft_get", account, draftSelector(), [this, account, id](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account || id != draftContext()) return;
        if (ok && !olderDraftProjection(result)) {
            m_draft = mergedDraftProjection(result); restoreRecipientInputs(m_draft); m_draftDirty = false; m_draftConflict = false;
            m_afterDraftSave = {}; m_draftStatus = draftReadOnly() ? "Read-only Google draft preview" : "Saved on this machine";
        } else if (!ok) { m_draftStatus = "Not saved — could not reload. Your version remains here."; cancelShutdown(); }
        else if (draftReadOnly()) m_draftStatus = "Read-only Google draft preview";
        emit draftChanged();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::saveMyDraftVersion() {
    // Called only after the user confirms replacement of the other saved
    // version. Fetch its revision, retaining every authored field locally, then
    // use the normal optimistic save. A concurrent edit can conflict again.
    if (accountArchived() || m_unsavedAccountBuffer || draftReadOnly() || !m_draftConflict || draftBusy() || m_draft.isEmpty()) return;
    m_draftTimer.stop(); clearError(); m_resolvingDraft = true;
    m_draftStatus = "Checking the current saved revision…"; emit draftChanged();
    const auto account = m_account, id = draftContext();
    request("draft_get", account, draftSelector(), [this, account, id](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account || id != draftContext()) return;
        if (!ok) { m_draftStatus = "Not saved — could not check the current revision. Your version remains here."; cancelShutdown(); emit draftChanged(); return; }
        // Another client may have made the first legacy save against a newer
        // member. Authored-field replacement cannot retarget frozen context.
        for (const auto &key : {"draft_id", "conversation_id", "mode", "source_message_id", "source_hash"}) {
            if (m_draft.value(key).toString() != result[key].toString()) {
                m_draftStatus = "Not saved — the saved draft has a different original message or composition identity. Your version remains here. Reload the saved draft only to replace these edits.";
                cancelShutdown(); emit draftChanged(); return;
            }
        }
        m_draft["revision"] = result["revision"].toVariant();
        mergeGoogleDraftStatus(result["google"].toObject(), account, result["draft_id"].toString());
        mergeGoogleSendStatus(result["google_send"].toObject(), account, result["draft_id"].toString());
        m_draftConflict = false; m_draftDirty = true; m_draftStatus = "Saving your version…"; emit draftChanged();
        saveDraft();
    });
}
void MailBackend::loadCaptures(bool append) {
    if (!m_connected || m_account.isEmpty()) return;
    if (append && m_captureNextOffset < 0) return;
    const auto account = m_account;
    const int generation = ++m_captureGeneration;
    request("capture_history", account, {{"limit", 100}, {"offset", append ? m_captureNextOffset : 0}}, [this, account, append, generation](auto result, bool ok) {
        if (!ok || account != m_account || generation != m_captureGeneration) return;
        const auto previous = m_captures;
        if (!append) m_captures.clear();
        for (const auto &value : result["captures"].toArray()) {
            const auto row = value.toObject().toVariantMap();
            bool exists = false;
            for (const auto &old : m_captures) if (old.toMap()["operation_id"] == row["operation_id"]) { exists = true; break; }
            if (!exists) m_captures.append(row);
        }
        if (!append) for (const auto &value : previous) {
            bool exists = false;
            for (const auto &current : m_captures) if (current.toMap()["operation_id"] == value.toMap()["operation_id"]) { exists = true; break; }
            if (!exists) m_captures.append(value);
        }
        m_captureNextOffset = result["total"].toInteger() > m_captures.size() ? int(m_captures.size()) : -1;
        emit capturesChanged();
    });
}
void MailBackend::loadMoreCaptures() { loadCaptures(true); }
void MailBackend::inspectCapture(const QString &operationId) {
    if (draftReadOnly() || draftBusy()) return;
    QVariantMap record;
    for (const auto &value : m_captures) if (value.toMap()["operation_id"].toString() == operationId) { record = value.toMap(); break; }
    if (record.isEmpty()) return;
    m_capturing = true; emit draftChanged();
    const auto account = m_account;
    QJsonObject params{{"request_id", operationId},
        {"expected_revision", record["draft_revision"].toLongLong()}, {"outcome", record.value("synthetic_outcome", "accepted").toString()}};
    if (record["draft_id"].toString().isEmpty()) params["conversation_id"] = record["conversation_id"].toString();
    else params["draft_id"] = record["draft_id"].toString();
    request("draft_capture", account, params, [this, account, operationId](auto result, bool ok) {
        m_capturing = false;
        if (ok) m_captureStatus = "Local capture " + operationId + ": " + result["state"].toString() + ". The original operation was reused; no email was delivered.";
        else { m_captureStatus = "Could not inspect the recorded operation: " + result["message"].toString(); cancelShutdown(); }
        if (ok) for (auto &value : m_captures) {
            auto row = value.toMap();
            if (row["operation_id"].toString() == operationId) { row["state"] = result["state"].toVariant(); value = row; }
        }
        emit capturesChanged(); emit draftChanged(); loadCaptures();
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::captureDraft() {
    if (attachmentImportPending()) return;
    if (draftLocked() || m_draft.isEmpty()) return;
    if (m_draftDirty || m_savingDraft) { m_afterDraftSave = [this] { captureDraft(); }; saveDraft(); return; }
    if (m_captureRequest.isEmpty() || m_captureAccount != m_account || m_captureConversation != draftContext()) {
        // The journal survives GUI restart. Resolve unfinished operations before
        // creating another identity, including records beyond normal history pages.
        m_capturing = true; m_captureStatus = "Checking existing local captures…"; emit draftChanged(); emit capturesChanged();
        const auto account = m_account, id = draftContext();
        auto selector = draftSelector();
        const auto draftId = m_draft.value("draft_id", m_selected).toString();
        auto history = QJsonObject{{"draft_id", draftId}, {"unresolved_only", true}, {"limit", 1}, {"offset", 0}};
        request("capture_history", account, history, [this, account, id, selector](auto result, bool ok) {
            m_capturing = false;
            if (!ok || account != m_account || id != draftContext()) {
                m_captureStatus = "Could not check existing captures. No new capture was created. Reconnect and retry.";
                cancelShutdown(); emit draftChanged(); emit capturesChanged(); return;
            }
            const auto unresolved = result["captures"].toArray();
            if (!unresolved.isEmpty()) {
                const auto record = unresolved.first().toObject();
                m_captureRequest = {{"request_id", record["operation_id"]},
                    {"expected_revision", record["draft_revision"]}, {"outcome", record["synthetic_outcome"]}};
                m_captureRequest["draft_id"] = record["draft_id"];
            } else {
                m_captureRequest = mutation();
                m_captureRequest["expected_revision"] = m_draft["revision"].toLongLong();
                for (auto it = selector.begin(); it != selector.end(); ++it) m_captureRequest[it.key()] = it.value();
            }
            m_captureAccount = account; m_captureConversation = id;
            captureDraft();
        });
        return;
    }
    m_capturing = true; m_captureStatus = "Capturing locally…"; emit capturesChanged(); emit draftChanged();
    const auto captureRevision = m_captureRequest["expected_revision"].toInteger();
    const bool earlierRevision = captureRevision != m_draft["revision"].toLongLong();
    request("draft_capture", m_captureAccount, m_captureRequest, [this, captureRevision, earlierRevision](auto result, bool ok) {
        m_capturing = false;
        if (ok) {
            const auto state = result["state"].toString("recorded");
            const bool uncertain = state == "acceptance_unknown" || state == "in_flight";
            m_captureStatus = "Local capture: " + state + " — no email was delivered";
            if (earlierRevision) m_captureStatus += QString(". Checked earlier saved revision %1; your current draft was not captured or changed.").arg(captureRevision);
            if (uncertain) m_captureStatus += ". Retry checks the original saved revision and operation; it does not submit another copy.";
            else m_captureRequest = {};
        } else {
            m_captureStatus = "Capture not confirmed. Retry keeps the same operation ID; your saved draft is retained.";
            cancelShutdown();
            if (result["code"].toString() == "stale_revision" || result["code"].toString() == "invalid_request") m_captureRequest = {};
        }
        emit capturesChanged(); emit draftChanged(); loadCaptures();
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}
