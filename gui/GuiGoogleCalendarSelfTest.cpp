// Developer harness for `--google-calendar-self-test <report> --google-calendar-expectation
// <file>`, run by scripts/verify_google_calendar_gui.py and verify_google_occurrences.py:
// Calendar create, guest edits, recurring edits and delete against a provider fixture.
#include "GuiGoogleCalendarSelfTest.h"
#include "MailBackend.h"
#include <QCoreApplication>
#include <QDate>
#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJSValue>
#include <QMouseEvent>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSaveFile>
namespace {
const QStringList quotaKeys{"gmail_project_units_per_minute", "gmail_account_units_per_minute",
    "calendar_project_units_per_minute", "calendar_account_units_per_minute"};
QQuickItem *named(QQuickItem *root, const QString &name) {
    if (!root) return nullptr;
    if (root->objectName() == name) return root;
    for (auto child : root->childItems()) if (auto found = named(child, name)) return found;
    return nullptr;
}
QVariant plain(QVariant value) { return value.canConvert<QJSValue>() ? value.value<QJSValue>().toVariant() : value; }
// The header New button is gone; Calendar's own entry point (also its n key) opens a new event.
bool createEvent(QQuickWindow *window) {
    auto workspace = window ? named(window->contentItem(), "calendarWorkspace") : nullptr;
    return workspace && workspace->isVisible() && QMetaObject::invokeMethod(workspace, "createEvent");
}
QJsonObject object(QObject *item, const char *property) { return item ? QJsonObject::fromVariantMap(plain(item->property(property)).toMap()) : QJsonObject{}; }
QStringList addresses(const QJsonValue &value) {
    QStringList result;
    for (const auto &entry : value.toArray()) result << entry.toString().trimmed().toLower();
    result.sort(); return result;
}
bool matchesOccurrence(const QJsonObject &row, const QString &account, const QJsonObject &selector) {
    const auto key = row.value("occurrence_key").toObject(), original = selector.value("original_start").toObject();
    if (row.value("account") != account || row.value("calendar_id") != selector.value("calendar_id")
        || key.value("series") != selector.value("series_id")) return false;
    if (original.value("kind") == "date") return row.value("all_day").toBool() && key.value("original_start") == original.value("date");
    const auto wanted = QDateTime::fromString(original.value("instant").toString(), Qt::ISODate);
    const auto actual = QDateTime::fromString(key.value("original_start").toString(), Qt::ISODate);
    return original.value("kind") == "zoned" && !row.value("all_day").toBool() && wanted.isValid() && actual.isValid() && wanted == actual;
}
bool sameOriginal(const QJsonObject &left, const QJsonObject &right) {
    if (left.value("kind") != right.value("kind")) return false;
    if (left.value("kind") == "date") return left.value("date") == right.value("date");
    const auto a = QDateTime::fromString(left.value("instant").toString(), Qt::ISODate), b = QDateTime::fromString(right.value("instant").toString(), Qt::ISODate);
    return left.value("kind") == "zoned" && a.isValid() && b.isValid() && a == b;
}
}
GuiGoogleCalendarSelfTest::GuiGoogleCalendarSelfTest(MailBackend &backend, QQuickWindow *window,
        const QString &expectation, const QString &report, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_report(report) {
    if (QFileInfo::exists(report)) {
        m_finished = true; qCritical() << "Refusing native Calendar report overwrite" << report;
        QTimer::singleShot(0, this, [] { QCoreApplication::exit(5); }); return;
    }
    QFile input(expectation);
    if (!input.open(QIODevice::ReadOnly) || input.size() > 1024 * 1024) {
        QTimer::singleShot(0, this, [this] { finish("Missing or oversized Calendar expectation."); }); return;
    }
    const auto expected = QJsonDocument::fromJson(input.readAll()).object();
    m_recurringMode = expected.value("scenario") == "recurring";
    m_occurrenceMode = m_recurringMode || expected.value("scenario") == "occurrence";
    m_expected = expected["accounts"].toArray(); m_limits = expected["limits"].toObject(); m_date = expected["calendar_date"].toString();
    if (!window || expected["version"] != 1 || m_expected.isEmpty() || m_expected.size() > 4
        || !QDate::fromString(m_date, Qt::ISODate).isValid() || m_limits.size() != quotaKeys.size()) {
        QTimer::singleShot(0, this, [this] { finish("Invalid native Calendar expectation."); }); return;
    }
    for (const auto &key : quotaKeys) {
        const auto value = m_limits[key].toInteger(-1);
        if (value < 100 || value > 100000000 || m_limits[key].toDouble() != double(value)) {
            QTimer::singleShot(0, this, [this] { finish("Invalid independently supplied sync limit."); }); return;
        }
    }
    for (const auto &value : m_expected) {
        const auto row = value.toObject(), event = row["event"].toObject(), fresh = row["new_event"].toObject();
        if (m_occurrenceMode) {
            const auto actions = row.value("actions").toArray();
            if (row.value("account").toString().isEmpty() || row.value("calendar_id").toString().isEmpty() || actions.isEmpty() || actions.size() > 8 || row.value("absent_selectors").toArray().size() > 8) {
                QTimer::singleShot(0, this, [this] { finish("Incomplete native occurrence expectation."); }); return;
            }
            for (const auto &entry : actions) {
                const auto action = entry.toObject(), selector = action.value("selector").toObject();
                const auto kind = action.value("kind").toString(), scope = action.value("scope").toString("occurrence");
                const bool create = m_recurringMode && kind == "create", dynamic = m_recurringMode && !action.value("series_from").toString().isEmpty();
                const auto fields = action.value("fields").toObject();
                bool valid = QDate::fromString(action.value("date").toString(), Qt::ISODate).isValid();
                if (create) valid = valid && !action.value("save_as").toString().isEmpty() && !fields.value("title").toString().isEmpty()
                    && fields.value("recurrence").toArray() == QJsonArray{"RRULE:FREQ=DAILY;COUNT=3"}
                    && QDateTime::fromString(fields.value("start").toString(), Qt::ISODate).isValid()
                    && QDateTime::fromString(fields.value("end").toString(), Qt::ISODate).isValid()
                    && fields.value("time_zone") == "UTC";
                else valid = valid && QStringList{"edit", "delete", "rsvp"}.contains(kind)
                    && (kind != "rsvp" || m_recurringMode) && QStringList{"occurrence", "series", "following"}.contains(scope)
                    && selector.value("calendar_id") == row.value("calendar_id")
                    && (dynamic || (!selector.value("series_id").toString().isEmpty() && !action.value("event_id").toString().isEmpty()))
                    && !selector.value("original_start").toObject().isEmpty()
                    && (kind != "edit" || (fields.size() == 1 && !fields.value("title").toString().isEmpty()))
                    && (kind != "rsvp" || QStringList{"accepted", "declined", "tentative"}.contains(action.value("response").toString()));
                if (!valid) { QTimer::singleShot(0, this, [this] { finish("Unsupported native recurring action expectation."); }); return; }
            }
            continue;
        }
        if (row["account"].toString().isEmpty() || event["event_id"].toString().isEmpty() || event["calendar_id"].toString().isEmpty()
            || row["add_guest"].toString().isEmpty() || fresh["title"].toString().isEmpty() || fresh["attendees"].toArray().isEmpty()
            || fresh["start"].toString().size() < 16 || fresh["end"].toString().size() < 16 || fresh["time_zone"].toString().isEmpty()) {
            QTimer::singleShot(0, this, [this] { finish("Incomplete Calendar action expectation."); }); return;
        }
    }
    window->resize(1280, 900); window->setProperty("workspace", "mail"); window->setProperty("settingsOpen", false);
    connect(&backend, &MailBackend::calendarWriteFinished, this, [this](bool ok, const QVariantMap &result, const QString &error) {
        if (m_finished || result.value("account").toString() != m_request["account"].toString()
            || result.value("request_id").toString() != m_request["request_id"].toString()) return;
        m_response = QJsonObject::fromVariantMap(result); m_responseOk = ok; m_responseError = error;
    });
    connect(&backend, &MailBackend::calendarOccurrenceReadFinished, this, [this](const QString &, bool ok, const QVariantMap &result, const QString &) {
        if (!m_occurrenceMode || !ok || m_finished) return;
        const auto account = m_expected[m_account].toObject();
        const auto actions = account.value("actions").toArray(); if (m_occurrenceIndex >= actions.size()) return;
        const auto action = actions[m_occurrenceIndex].toObject();
        if (result.value("account").toString() == account.value("account").toString()
            && result.value("calendar_id").toString() == account.value("calendar_id").toString()
            && (action.value("event_id").toString().isEmpty() || result.value("event_id").toString() == action.value("event_id").toString()))
            m_occurrenceResponse = QJsonObject::fromVariantMap(result);
    });
    if (auto details = window->findChild<QObject *>("eventDetails"))
        connect(details, SIGNAL(pendingResponseChanged()), this, SLOT(captureResponseRequest()));
    m_timer.setInterval(50); connect(&m_timer, &QTimer::timeout, this, &GuiGoogleCalendarSelfTest::tick);
    m_watchdog.setSingleShot(true); connect(&m_watchdog, &QTimer::timeout, this, [this] {
        finish(QString("Native Calendar timed out at stage %1, account %2, action %3: %4")
            .arg(m_stage).arg(m_account).arg(m_action, m_backend.calendarOperationStatus()));
    });
    m_watchdog.start(120000); m_timer.start();
}
bool GuiGoogleCalendarSelfTest::click(const QString &name) {
    if (!m_window) return false;
    auto item = named(m_window->contentItem(), name);
    if (!item || !item->isVisible() || !item->isEnabled() || item->width() <= 0 || item->height() <= 0) return false;
    for (auto ancestor = item->parentItem(); ancestor; ancestor = ancestor->parentItem()) {
        if (!ancestor->property("contentY").isValid()) continue;
        const auto p = item->mapToItem(ancestor, {item->width() / 2, item->height() / 2});
        if (p.y() >= 8 && p.y() < ancestor->height() - 8) continue;
        const double origin = ancestor->property("originY").toDouble();
        const double maximum = qMax(origin, origin + ancestor->property("contentHeight").toDouble() - ancestor->height());
        const double next = qBound(origin, ancestor->property("contentY").toDouble() + p.y() - ancestor->height() / 2, maximum);
        if (qAbs(next - ancestor->property("contentY").toDouble()) > .5) { ancestor->setProperty("contentY", next); return false; }
    }
    QPointer<QQuickItem> control = item;
    const auto p = item->mapToScene({item->width() / 2, item->height() / 2});
    if (!QRectF(0, 0, m_window->width(), m_window->height()).contains(p)) return false;
    const QPointF global = m_window->mapToGlobal(p.toPoint());
    QMouseEvent move(QEvent::MouseMove, p, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &move); if (!control) return false;
    QMouseEvent press(QEvent::MouseButtonPress, p, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &press);
    QMouseEvent release(QEvent::MouseButtonRelease, p, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &release); return true;
}
bool GuiGoogleCalendarSelfTest::openEvent(const QString &account, const QString &eventId) {
    auto workspace = named(m_window->contentItem(), "calendarWorkspace"), agenda = named(m_window->contentItem(), "calendarAgenda");
    if (!workspace || !agenda || !m_backend.calendarReady()) return false;
    const auto rows = plain(workspace->property("agendaEvents")).toList();
    for (int index = 0; index < rows.size(); ++index) {
        const auto row = rows[index].toMap();
        if (row.value("account").toString() != account || row.value("event_id").toString() != eventId) continue;
        const QString name = "calendarAgendaEvent-" + account + "-" + eventId;
        if (!named(m_window->contentItem(), name)) {
            QMetaObject::invokeMethod(agenda, "positionViewAtIndex", Q_ARG(int, index), Q_ARG(int, 4)); return false;
        }
        return click(name);
    }
    return false;
}
bool GuiGoogleCalendarSelfTest::submit(const QString &action, int nextStage) {
    if (action == "delete" || action == "following_edit") {
        const auto name = action == "delete" ? "eventDeleteConfirmation" : "eventFollowingConfirmation";
        const auto dialog = m_window->findChild<QObject *>(name);
        if (!dialog || !dialog->property("opened").toBool()) return false;
    }
    m_response = {}; m_responseError.clear(); m_responseOk = false;
    if (!click(action == "delete" ? "eventConfirmDelete" : action == "following_edit" ? "eventConfirmFollowing" : "eventSaveButton")) return false;
    m_request = object(m_window->findChild<QObject *>("eventEditor"), "submittedRequest");
    if (m_request["request_id"].toString().isEmpty()) { finish("Actual Calendar control did not submit a request."); return false; }
    m_action = action; m_nextStage = nextStage; m_stage = m_occurrenceMode ? 106 : 13; return true;
}
void GuiGoogleCalendarSelfTest::tick() {
    if (m_finished || !m_window) return;
    if (m_loggedStage != m_stage) { m_loggedStage = m_stage; qInfo() << "Native Calendar stage" << m_stage << "account" << m_account << "action" << m_action; }
    if (!m_backend.error().isEmpty()) { finish(m_backend.error()); return; }
    const auto account = m_expected[m_account].toObject(), expected = account["event"].toObject();
    const QString id = account["account"].toString(), eventId = expected["event_id"].toString(), calendar = expected["calendar_id"].toString();
    auto editor = m_window->findChild<QObject *>("eventEditor");
    if (m_occurrenceMode && m_stage >= 100) { occurrenceTick(account, editor); return; }
    const auto eventFor = [&](const QString &key) {
        for (const auto &value : m_backend.events()) {
            const auto row = QJsonObject::fromVariantMap(value.toMap());
            if (row["account"] == id && row["calendar_id"] == calendar && row["event_id"] == key) return row;
        }
        return QJsonObject{};
    };
    switch (m_stage) {
    case 0:
        if (const auto loader = named(m_window->contentItem(), "startupOverlay"); loader && loader->isVisible()) return;
        if (!m_backend.connected() || m_backend.accounts().isEmpty()) return;
        if (m_backend.syntheticProfile()) { finish("Calendar actions require the isolated live-mode fixture."); return; }
        m_stage = 1; break;
    case 1: if (click("accountTab_" + id)) m_stage = 2; break;
    case 2:
        if (m_backend.account() != id || m_backend.loading() || m_backend.draftBusy()) return;
        if (m_backend.googleSyncState(id).value("owned").toBool()) { finish("Unexpected GUI owner before explicit Settings start."); return; }
        if (click("settingsButton")) m_stage = 3; break;
    case 3: if (click("settingsPage_Accounts")) m_stage = 4; break;
    case 4: if (click("googleSync_" + id)) { m_stage = 5; } break;
    case 5: {
        if (!m_backend.googleSyncState(id).value("owned").toBool()) return;
        if (m_window->findChild<QObject *>("googleSyncLimitsDialog")) { finish("Removed quota setup is still present."); return; }
        if (QJsonObject::fromVariantMap(m_backend.googleSyncLimits()) != m_limits) { finish("Actual native limits differ from independent expectation."); return; }
        m_accountResult = {{"account", id}, {"limits", m_limits}}; m_createdId.clear(); m_createdSeries = {};
        if (click("settingsDoneButton")) { m_backend.setCalendarDate(m_date); m_stage = 6; } break;
    }
    case 6: if (click("calendarWorkspaceButton")) m_stage = 7; break;
    case 7: if (click("calendarMoreButton")) m_stage = 8; break;
    case 8: if (click("calendarAgendaButton")) m_stage = 9; break;
    case 9:
        if (m_occurrenceMode) { m_occurrenceIndex = 0; m_stage = 100; break; }
        if (eventFor(eventId).isEmpty()) return;
        m_initial = eventFor(eventId);
        for (auto it = expected.begin(); it != expected.end(); ++it) if (m_initial[it.key()] != it.value()) { finish("Initial native event differs at " + it.key()); return; }
        if (!m_initial["can_edit_attendees"].toBool()) { finish("Ordinary organizer fixture lacks guest-edit capability."); return; }
        m_accountResult["initial_event"] = m_initial;
        if (openEvent(id, eventId)) m_stage = 10; break;
    case 10: if (click("eventEditButton")) m_stage = 11; break;
    case 11: {
        if (!editor || !editor->property("opened").toBool()) return;
        auto field = named(m_window->contentItem(), "eventAttendees");
        if (!field || field->property("readOnly").toBool()) { finish("Actual guest editor is unavailable."); return; }
        field->setProperty("text", account["add_guest"].toString()); QMetaObject::invokeMethod(field, "accepted");
        m_stage = 12; break;
    }
    case 12: submit("add", 14); break;
    case 13: {
        if (m_response.isEmpty()) return;
        if (!m_responseOk || !QStringList{"accepted", "acknowledged"}.contains(m_response["state"].toString())) { finish("Expected actual provider acceptance: " + m_responseError); return; }
        if (editor && editor->property("visible").toBool()) return;
        QJsonObject current;
        if (m_action == "create") {
            const auto fresh = account["new_event"].toObject();
            for (const auto &value : m_backend.events()) {
                const auto row = QJsonObject::fromVariantMap(value.toMap());
                if (row["account"] == id && row["calendar_id"] == calendar && row["title"] == fresh["title"]) { current = row; m_createdId = row["event_id"].toString(); break; }
            }
            if (current.isEmpty()) return;
            if (addresses(current["guest_addresses"]) != addresses(fresh["attendees"])) { finish("Created event guest set differs from authored input."); return; }
        } else if (m_action == "delete") {
            if (!eventFor(m_createdId).isEmpty()) return;
        } else {
            current = eventFor(eventId);
            if (current.isEmpty() || current["etag"] == m_request["expected_etag"]) return;
            auto wanted = addresses(m_initial["guest_addresses"]);
            if (m_action == "add") { wanted << account["add_guest"].toString().trimmed().toLower(); wanted.sort(); }
            if (addresses(current["guest_addresses"]) != wanted) { finish("Observed edited event guest membership differs."); return; }
            for (const auto field : {"title", "location", "description", "start", "end", "all_day", "conference_url", "protected_attendees"})
                if (current[field] != m_initial[field]) { finish("Guest-only edit changed native retained field " + QString(field)); return; }
        }
        QJsonObject result{{"request", m_request}, {"outcome", m_response}};
        if (!current.isEmpty()) result["event"] = current;
        m_accountResult[m_action] = result; m_stage = m_nextStage; break;
    }
    case 14: if (openEvent(id, eventId)) m_stage = 15; break;
    case 15: if (click("eventEditButton")) m_stage = 16; break;
    case 16: {
        if (!editor || !editor->property("opened").toBool()) return;
        const auto guests = plain(editor->property("guestAddresses")).toStringList();
        const int index = guests.indexOf(account["add_guest"].toString());
        if (index < 0) { finish("Added guest chip was not restored from Google."); return; }
        if (click("eventRemoveGuest-" + QString::number(index))) m_stage = 17; break;
    }
    case 17: submit("remove", 21); break;
    case 21: if (createEvent(m_window)) m_stage = 22; break;
    case 22: {
        if (!editor || !editor->property("opened").toBool()) return;
        const auto fresh = account["new_event"].toObject();
        auto target = named(m_window->contentItem(), "eventTargetCalendar");
        const auto sources = plain(editor->property("writableSources")).toList(); int selected = -1;
        for (int index = 0; index < sources.size(); ++index) {
            const auto source = sources[index].toMap();
            if (source.value("account").toString() == id && source.value("calendar_id").toString() == calendar) { selected = index; break; }
        }
        if (!target || selected < 0) { finish("Expected create calendar is unavailable."); return; }
        target->setProperty("currentIndex", selected); QMetaObject::invokeMethod(target, "activated", Q_ARG(int, selected));
        const QMap<QString, QString> text{{"eventTitle", fresh["title"].toString()}, {"eventStartDate", fresh["start"].toString().left(10)},
            {"eventEndDate", fresh["end"].toString().left(10)}, {"eventStartTime", fresh["start"].toString().mid(11, 5)},
            {"eventEndTime", fresh["end"].toString().mid(11, 5)}, {"eventTimeZone", fresh["time_zone"].toString()},
            {"eventLocation", fresh["location"].toString()}, {"eventDescription", fresh["description"].toString()},
            {"eventAttendees", addresses(fresh["attendees"]).join(", ")}};
        for (auto it = text.begin(); it != text.end(); ++it) {
            auto field = named(m_window->contentItem(), it.key());
            if (!field || !field->setProperty("text", it.value())) { finish("Could not author native event field " + it.key()); return; }
        }
        m_stage = 23; break;
    }
    case 23: submit("create", 24); break;
    case 24: if (openEvent(id, m_createdId)) m_stage = 25; break;
    case 25: if (click("eventEditButton")) m_stage = 26; break;
    case 26: if (click("eventDeleteButton")) m_stage = 27; break;
    case 27: submit("delete", 28); break;
    case 28: if (click("settingsButton")) m_stage = 29; break;
    case 29: if (click("settingsPage_Accounts")) m_stage = 30; break;
    case 30: if (click("googleSync_" + id)) m_stage = 31; break;
    case 31:
        if (m_backend.googleSyncState(id).value("owned").toBool() || m_backend.googleSyncState(id).value("changing").toBool()) return;
        if (click("settingsDoneButton")) m_stage = 32; break;
    case 32:
        if (!click("mailWorkspaceButton")) return;
        m_observed.append(m_accountResult); m_accountResult = {};
        if (++m_account >= m_expected.size()) { finish(); return; }
        m_stage = 1; break;
    }
}
QJsonObject GuiGoogleCalendarSelfTest::occurrenceFor(const QString &account, const QJsonObject &selector) const {
    for (const auto &value : m_backend.events()) {
        const auto row = QJsonObject::fromVariantMap(value.toMap());
        if (matchesOccurrence(row, account, selector)) return row;
    }
    return {};
}
bool GuiGoogleCalendarSelfTest::openOccurrence(const QString &account, const QJsonObject &selector) {
    auto workspace = named(m_window->contentItem(), "calendarWorkspace"), agenda = named(m_window->contentItem(), "calendarAgenda");
    if (!workspace || !agenda || !m_backend.calendarReady()) return false;
    const auto rows = plain(workspace->property("agendaEvents")).toList();
    for (int index = 0; index < rows.size(); ++index) {
        const auto row = QJsonObject::fromVariantMap(rows[index].toMap());
        if (!matchesOccurrence(row, account, selector)) continue;
        const auto name = "calendarAgendaOccurrence-" + row.value("id").toString();
        if (!named(m_window->contentItem(), name)) { QMetaObject::invokeMethod(agenda, "positionViewAtIndex", Q_ARG(int, index), Q_ARG(int, 4)); return false; }
        return click(name);
    }
    return false;
}
QJsonObject GuiGoogleCalendarSelfTest::actionSelector(const QJsonObject &action) const {
    auto selector = action.value("selector").toObject();
    const auto alias = action.value("series_from").toString();
    if (!alias.isEmpty()) selector["series_id"] = m_createdSeries.value(alias);
    return selector;
}
void GuiGoogleCalendarSelfTest::captureResponseRequest() {
    if (!m_occurrenceMode || m_finished || m_stage != 102) return;
    const auto action = m_expected[m_account].toObject().value("actions").toArray()[m_occurrenceIndex].toObject();
    if (action.value("kind") != "rsvp") return;
    const auto request = object(m_window->findChild<QObject *>("eventDetails"), "pendingResponse");
    if (request.value("request_id").toString().isEmpty()) return;
    // Capture the actual property assignment before the asynchronous write can
    // finish and clear it. A cached status is never substituted for an action.
    m_request = request; m_response = {}; m_responseError.clear(); m_responseOk = false; m_action = "rsvp";
}
bool GuiGoogleCalendarSelfTest::authorCreate(const QString &account, const QString &calendar, const QJsonObject &fields, QObject *editor) {
    auto target = named(m_window->contentItem(), "eventTargetCalendar");
    const auto sources = plain(editor->property("writableSources")).toList(); int selected = -1;
    for (int index = 0; index < sources.size(); ++index) {
        const auto source = sources[index].toMap();
        if (source.value("account").toString() == account && source.value("calendar_id").toString() == calendar) { selected = index; break; }
    }
    if (!target || selected < 0) { finish("Expected recurring create calendar is unavailable."); return false; }
    target->setProperty("currentIndex", selected); QMetaObject::invokeMethod(target, "activated", Q_ARG(int, selected));
    const QMap<QString, QString> text{{"eventTitle", fields["title"].toString()}, {"eventStartDate", fields["start"].toString().left(10)},
        {"eventEndDate", fields["end"].toString().left(10)}, {"eventStartTime", fields["start"].toString().mid(11, 5)},
        {"eventEndTime", fields["end"].toString().mid(11, 5)}, {"eventTimeZone", fields["time_zone"].toString()},
        {"eventLocation", fields["location"].toString()}, {"eventDescription", fields["description"].toString()},
        {"eventAttendees", addresses(fields["attendees"]).join(", ")}};
    for (auto it = text.begin(); it != text.end(); ++it) {
        auto field = named(m_window->contentItem(), it.key());
        if (!field || !field->setProperty("text", it.value())) { finish("Could not author native recurring field " + it.key()); return false; }
    }
    auto repeat = named(m_window->contentItem(), "eventRepeat"), interval = named(m_window->contentItem(), "eventRepeatInterval"), count = named(m_window->contentItem(), "eventRepeatCount");
    if (!repeat || !interval || !count || !repeat->setProperty("currentIndex", 1)
        || !QMetaObject::invokeMethod(repeat, "activated", Q_ARG(int, 1)) || !interval->setProperty("text", "1") || !count->setProperty("text", "3")) {
        finish("Actual Repeat controls are unavailable."); return false;
    }
    return true;
}
void GuiGoogleCalendarSelfTest::occurrenceTick(const QJsonObject &account, QObject *editor) {
    const auto id = account.value("account").toString(), calendar = account.value("calendar_id").toString();
    const auto actions = account.value("actions").toArray(); const auto action = actions[m_occurrenceIndex].toObject();
    const auto selector = actionSelector(action); const auto kind = action.value("kind").toString();
    const auto scope = action.value("scope").toString("occurrence");
    auto details = m_window->findChild<QObject *>("eventDetails");
    switch (m_stage) {
    case 100: {
        if (!m_backend.calendarReady()) return;
        if (m_occurrenceIndex == 0 && !m_accountResult.contains("initial_events")) {
            if (m_backend.calendarFrom() != m_date || m_backend.calendarDays() != 42) return;
            bool complete = false;
            for (const auto &value : m_backend.calendarSources()) { const auto source = value.toMap(); if (source.value("account") == id && source.value("calendar_id") == calendar && source.value("coverage") == "cached") complete = true; }
            if (!complete) return;
            for (const auto &entry : actions) {
                const auto initialAction = entry.toObject();
                if (initialAction.value("kind") != "create" && initialAction.value("series_from").toString().isEmpty()
                    && occurrenceFor(id, initialAction.value("selector").toObject()).isEmpty()) return;
            }
            QJsonArray initial, absent;
            for (const auto &value : m_backend.events()) { const auto row = QJsonObject::fromVariantMap(value.toMap()); if (row.value("account") == id && row.value("calendar_id") == calendar) initial.append(row); }
            for (const auto &entry : account.value("absent_selectors").toArray()) {
                const auto excluded = entry.toObject(), start = excluded.value("original_start").toObject();
                const auto date = QDate::fromString(start.value("kind") == "date" ? start.value("date").toString() : start.value("instant").toString().left(10), Qt::ISODate);
                const auto from = QDate::fromString(m_backend.calendarFrom(), Qt::ISODate);
                if (!date.isValid() || date < from || date >= from.addDays(m_backend.calendarDays()) || !occurrenceFor(id, excluded).isEmpty()) { finish("Expected sparse cancellation is visible or outside the inspected native window."); return; }
                absent.append(excluded);
            }
            m_accountResult["initial_events"] = initial; m_accountResult["observed_absent_selectors"] = absent;
            m_accountResult["calendar_from"] = m_backend.calendarFrom(); m_accountResult["calendar_days"] = m_backend.calendarDays();
        }
        if (m_backend.calendarDate() != action.value("date").toString()) { m_backend.setCalendarDate(action.value("date").toString()); return; }
        if (kind == "create") { if (createEvent(m_window)) { m_initial = {}; m_resolvedOccurrence = {}; m_occurrenceResponse = {}; m_stage = 108; } break; }
        if (selector.value("series_id").toString().isEmpty()) { finish("A dynamic recurring action has no prior created series."); return; }
        m_initial = occurrenceFor(id, selector); if (m_initial.isEmpty()) return;
        const auto scopes = m_initial.value("recurring_scopes").toObject();
        if (!scopes.isEmpty()) {
            if (!scopes.value(kind).toArray().contains(scope)) { finish("Selected recurring scope does not permit the expected action."); return; }
        } else if (m_recurringMode || !m_initial.value("can_resolve_occurrence").toBool()) { finish("Expected occurrence is not eligible for provider resolution."); return; }
        if (openOccurrence(id, selector)) { m_occurrenceResponse = {}; m_resolvedOccurrence = {}; m_request = {}; m_stage = 101; } break;
    }
    case 101: {
        if (!details || !details->property("opened").toBool()) return;
        if (!m_initial.value("recurring_scopes").toObject().isEmpty()) {
            auto control = named(m_window->contentItem(), "eventRecurringScope");
            const auto choices = plain(details->property("recurringScopes")).toStringList(); const int index = choices.indexOf(scope);
            if (!control || index < 0 || !control->setProperty("currentIndex", index)
                || !QMetaObject::invokeMethod(control, "activated", Q_ARG(int, index))) { finish("Actual recurring scope control is unavailable."); return; }
        }
        const auto button = kind == "rsvp" ? "eventRsvp-" + action.value("response").toString()
            : kind == "delete" && !m_initial.value("recurring_scopes").toObject().isEmpty() ? QString("eventScopedCancel") : QString("eventEditButton");
        if (click(button)) m_stage = 102; break;
    }
    case 102: {
        if (kind == "rsvp") {
            if (m_occurrenceResponse.isEmpty() || m_request.isEmpty()) {
                const auto feedback = details ? details->property("feedback").toString() : QString{};
                if (!feedback.isEmpty() && !feedback.startsWith("Checking")) finish("Native scoped RSVP resolution failed: " + feedback);
                return;
            }
            m_resolvedOccurrence = m_occurrenceResponse.value("event").toObject();
            m_resolvedOccurrence["recurring_context"] = m_occurrenceResponse.value("recurring");
        } else {
            if (!editor || !editor->property("opened").toBool()) {
                const auto feedback = details ? details->property("feedback").toString() : QString{};
                if (!feedback.isEmpty() && !feedback.startsWith("Checking")) finish("Native occurrence resolution failed: " + feedback);
                return;
            }
            m_resolvedOccurrence = object(editor, "editEvent");
        }
        const auto recurring = m_resolvedOccurrence.value("recurring_context").toObject();
        const auto context = recurring.isEmpty() ? m_resolvedOccurrence.value("occurrence").toObject() : recurring;
        const bool series = recurring.value("scope") == "series";
        if (m_occurrenceResponse.isEmpty() || m_resolvedOccurrence.value("account") != id || m_resolvedOccurrence.value("calendar_id") != calendar
            || (!action.value("event_id").toString().isEmpty() && m_resolvedOccurrence.value("event_id") != action.value("event_id"))
            || context.value("series_id") != selector.value("series_id") || context.value("expected_master_etag").toString().isEmpty()
            || (!series && !sameOriginal(context.value("original_start").toObject(), selector.value("original_start").toObject()))
            || (series && (m_resolvedOccurrence.value("event_id") != selector.value("series_id") || context.contains("original_start")))) {
            finish("Native resolved recurring action has an unexpected identity."); return;
        }
        const auto expected = action.value("event").toObject();
        for (auto it = expected.begin(); it != expected.end(); ++it) if (m_resolvedOccurrence.value(it.key()) != it.value()) { finish("Resolved native occurrence differs at " + it.key()); return; }
        if (kind == "rsvp") { m_stage = 106; break; }
        if (kind == "delete") {
            if (!recurring.isEmpty()) m_stage = 104; // Scoped Cancel opens its existing confirmation on resolution.
            else if (click("eventDeleteButton")) m_stage = 104;
        } else {
            auto title = named(m_window->contentItem(), "eventTitle");
            if (!title || !title->setProperty("text", action.value("fields").toObject().value("title").toString())) { finish("Could not author occurrence title."); return; }
            m_stage = 103;
        }
        break;
    }
    case 103:
        if (scope == "following") { if (click("eventSaveButton")) m_stage = 105; }
        else submit("edit", 107);
        break;
    case 104: submit("delete", 107); break;
    case 105: submit("following_edit", 107); break;
    case 108:
        if (editor && editor->property("opened").toBool() && authorCreate(id, calendar, action.value("fields").toObject(), editor)) m_stage = 109;
        break;
    case 109: submit("create", 107); break;
    case 106: {
        if (m_response.isEmpty()) return;
        if (!m_responseOk || !QStringList{"accepted", "acknowledged"}.contains(m_response.value("state").toString())) { finish("Expected recurring provider acceptance: " + m_responseError); return; }
        if (editor && editor->property("visible").toBool()) return;
        QJsonObject current;
        if (kind == "create") {
            for (const auto &value : m_backend.events()) {
                const auto row = QJsonObject::fromVariantMap(value.toMap());
                if (row.value("account") == id && row.value("calendar_id") == calendar && row.value("title") == action.value("fields").toObject().value("title")) { current = row; break; }
            }
            if (current.isEmpty()) return;
            const auto series = current.value("occurrence_key").toObject().value("series").toString();
            if (series.isEmpty()) { finish("Recurring create did not produce an observed series."); return; }
            m_createdSeries[action.value("save_as").toString()] = series;
        } else if (scope == "following" && kind == "edit") {
            if (m_response.value("following_complete") != true) { finish("Following edit did not confirm both provider steps."); return; }
            QString replacement;
            for (const auto &step : m_response.value("steps").toArray()) if (step.toObject().value("step") == "replacement") replacement = step.toObject().value("target").toString();
            if (replacement.isEmpty()) { finish("Following receipt has no exact replacement target."); return; }
            auto replacementSelector = selector; replacementSelector["series_id"] = replacement;
            current = occurrenceFor(id, replacementSelector);
            if (current.isEmpty() || !occurrenceFor(id, selector).isEmpty()) return;
        } else current = occurrenceFor(id, selector);
        if (kind == "delete") { if (!current.isEmpty()) return; }
        else if (kind == "rsvp") {
            if (current.isEmpty() || current.value("self_response") != action.value("response")) return;
            if (details && details->property("visible").toBool()) { if (!click("eventDetailsClose")) return; }
        } else if (kind == "edit") {
            if (current.isEmpty() || current.value("title") != action.value("fields").toObject().value("title")) return;
            const auto before = scope == "series" ? m_initial : m_resolvedOccurrence;
            for (const auto field : {"start", "end", "all_day", "time_zone", "location", "description", "conference_url"})
                if (current.value(field) != before.value(field)) { finish("Recurring title edit changed native field " + QString(field)); return; }
        }
        QJsonArray postEvents;
        for (const auto &value : m_backend.events()) { const auto row = QJsonObject::fromVariantMap(value.toMap()); if (row.value("account") == id && row.value("calendar_id") == calendar) postEvents.append(row); }
        auto observed = m_accountResult.value("actions").toArray();
        QJsonObject result{{"account", id}, {"kind", action.value("kind")}, {"selector", selector}, {"selected_occurrence", m_initial},
            {"resolved_event", m_resolvedOccurrence}, {"resolved_response", m_occurrenceResponse}, {"request", m_request}, {"outcome", m_response},
            {"post_events", postEvents}, {"calendar_from", m_backend.calendarFrom()}, {"calendar_days", m_backend.calendarDays()}};
        if (!current.isEmpty()) result["event"] = current;
        if (kind == "create") { result["save_as"] = action.value("save_as"); result["created_series_id"] = m_createdSeries.value(action.value("save_as").toString()); }
        else if (!m_resolvedOccurrence.value("recurring_context").toObject().isEmpty()) result["scope"] = scope;
        observed.append(result); m_accountResult["actions"] = observed; m_stage = 107; break;
    }
    case 107: m_stage = ++m_occurrenceIndex < actions.size() ? 100 : 28; break;
    }
}
void GuiGoogleCalendarSelfTest::finish(const QString &error) {
    if (m_finished) return;
    m_finished = true; m_timer.stop(); m_watchdog.stop();
    const QJsonObject result{{"version", 1}, {"result", error.isEmpty() ? "passed" : "failed"}, {"error", error},
        {"scenario", m_recurringMode ? "recurring" : m_occurrenceMode ? "occurrence" : "ordinary"}, {"stage", m_stage}, {"gui_pid", QCoreApplication::applicationPid()}, {"accounts", m_observed}, {"in_progress", m_accountResult},
        {"last_request", m_request}, {"last_outcome", m_response}};
    QSaveFile file(m_report);
    if (QFileInfo::exists(m_report) || !file.open(QIODevice::WriteOnly) || file.write(QJsonDocument(result).toJson()) < 0 || !file.commit()) {
        qCritical() << "Cannot retain native Calendar report" << m_report; QCoreApplication::exit(5); return;
    }
    m_passed = error.isEmpty();
    if (!m_passed) { qCritical().noquote() << error; QCoreApplication::exit(5); return; }
    qInfo() << "Native Calendar guest add/remove, create/delete and explicit Settings owner shutdown passed.";
    m_backend.setProperty("selfTestCompleted", true); m_backend.shutdown();
}
