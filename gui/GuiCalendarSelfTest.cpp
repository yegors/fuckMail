// Developer harness: the calendar stage of `fuckmail-gui --self-test`. Creates,
// edits (including a stale-save conflict) and RSVPs to an event through the real
// controls, then checks views and the Mail timeline; see verifyRestartProjection.
#include "GuiCalendarSelfTest.h"
#include <QCoreApplication>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QQuickItem>
#include <QJSValue>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QJsonDocument>
#include <QJsonArray>
#include <QDebug>
#include <QDate>
#include <QSaveFile>
#include <QSet>

namespace {
QQuickItem *findVisible(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name && parent->isVisible()) return parent;
    for (auto child : parent->childItems()) if (auto found = findVisible(child, name)) return found;
    return nullptr;
}
void key(QObject *target, int value) {
    QKeyEvent press(QEvent::KeyPress, value, Qt::NoModifier), release(QEvent::KeyRelease, value, Qt::NoModifier);
    QCoreApplication::sendEvent(target, &press); QCoreApplication::sendEvent(target, &release);
}
bool unclipped(QQuickItem *field) {
    const auto center = field->mapToScene({field->width() / 2, field->height() / 2});
    for (auto ancestor = field->parentItem(); ancestor; ancestor = ancestor->parentItem())
        if (ancestor->clip() && !ancestor->mapRectToScene(ancestor->boundingRect()).contains(center)) return false;
    return true;
}
void reveal(QQuickItem *field) {
    for (auto ancestor = field->parentItem(); ancestor; ancestor = ancestor->parentItem()) {
        if (!ancestor->property("contentY").isValid() || !ancestor->property("contentHeight").isValid()) continue;
        const auto rect = field->mapRectToItem(ancestor, field->boundingRect());
        const double current = ancestor->property("contentY").toDouble();
        if (rect.top() < 4) ancestor->setProperty("contentY", qMax(0.0, current + rect.top() - 4));
        else if (rect.bottom() > ancestor->height() - 4) ancestor->setProperty("contentY", current + rect.bottom() - ancestor->height() + 4);
    }
}
QVariant variantValue(const QVariant &value) {
    return value.canConvert<QJSValue>() ? value.value<QJSValue>().toVariant() : value;
}
bool visibleTitle(QQuickItem *parent, const QString &title) {
    if (!parent->isVisible()) return false;
    if (parent->property("text").toString().contains(title) && unclipped(parent)) return true;
    for (auto child : parent->childItems()) if (visibleTitle(child,title)) return true;
    return false;
}
QQuickItem *headerTimeline(QQuickItem *parent) {
    // The strip's root carries the placement; its day pages hold the blocks.
    if (parent->isVisible() && parent->property("headerPosition").toBool()) return parent;
    for (auto child:parent->childItems()) if (auto found=headerTimeline(child)) return found;
    return nullptr;
}
QQuickItem *editableTimelineBlock(QQuickItem *parent) {
    const auto data=variantValue(parent->property("modelData")).toMap();
    if (parent->isVisible() && parent->property("eventColor").isValid() && parent->width()>0
        && parent->height()>0 && unclipped(parent) && data["can_edit"].toBool()
        && !data["recurring"].toBool() && !data["event_id"].toString().isEmpty()) return parent;
    for (auto child:parent->childItems()) if (auto found=editableTimelineBlock(child)) return found;
    return nullptr;
}
}

bool GuiCalendarSelfTest::verifyRestartProjection(MailBackend &backend, QQuickWindow *window,
    const QString &expectedPath, QString &error) {
    const auto reject = [&error](const QString &message) { error = message; return false; };
    QFile input(expectedPath);
    if (!QDir::isAbsolutePath(expectedPath) || !input.open(QIODevice::ReadOnly) || input.size()>65536)
        return reject("Cannot read the bounded calendar restart expectation.");
    QJsonParseError parse;
    const auto document = QJsonDocument::fromJson(input.readAll(),&parse);
    const auto expected = document.object()["event"].toObject();
    const auto eventId = expected["event_id"].toString();
    if (parse.error != QJsonParseError::NoError || eventId.isEmpty() || expected["etag"].toString().isEmpty()
        || expected["account"].toString() != "windscribe" || expected["calendar_id"].toString() != "primary")
        return reject("Calendar restart expectation has no valid source identity and version.");
    if (!window || !backend.connected() || !backend.calendarReady() || backend.calendarDate() != "2026-09-11"
        || backend.calendarView() != "day" || window->property("workspace").toString() != "calendar")
        return reject("Restarted GUI did not load the expected combined Calendar day.");
    const auto calendar = findVisible(window->contentItem(),"calendarWorkspace");
    if (!calendar) return reject("Restarted native Calendar surface is not visible.");
    const auto qmlRows = variantValue(calendar->property("visibleEvents")).toList();
    QJsonObject observed;
    const auto checkRows = [&](const QVariantList &rows, const QString &surface) {
        int matches = 0;
        QSet<QString> accounts;
        for (const auto &value : rows) {
            const auto row = value.toMap(); accounts.insert(row["account"].toString());
            if (row["event_id"].toString() != eventId) continue;
            ++matches;
            const auto object = QJsonObject::fromVariantMap(row);
            for (auto it = expected.begin(); it != expected.end(); ++it) {
                if (object[it.key()] != it.value()) return reject(surface + " changed calendar field " + it.key() + " after restart.");
                observed[it.key()] = object[it.key()];
            }
        }
        if (matches != 1) return reject(surface + " did not contain exactly one edited event after restart.");
        if (!accounts.contains("windscribe") || !accounts.contains("personal") || !accounts.contains("sideproject"))
            return reject(surface + " lost the three-account combined calendar scope.");
        return true;
    };
    if (!checkRows(backend.events(),"Restarted adapter") || !checkRows(qmlRows,"Restarted QML model")) return false;
    const auto delegate = findVisible(window->contentItem(),"calendarEvent-"+eventId);
    if (!delegate || delegate->width()<=0 || delegate->height()<=0 || !unclipped(delegate)
        || !QRectF(0,0,window->width(),window->height()).contains(delegate->mapToScene({delegate->width()/2,delegate->height()/2}))
        || !visibleTitle(delegate,expected["title"].toString()))
        return reject("Restarted native event delegate and edited title are not visible.");
    const auto displayed = QJsonObject::fromVariantMap(variantValue(delegate->property("modelData")).toMap());
    for (auto it = expected.begin(); it != expected.end(); ++it)
        if (displayed[it.key()] != it.value()) return reject("Restarted native event delegate changed field " + it.key());
    const QJsonObject report{{"event",observed},{"combined_accounts",QJsonArray{"windscribe","personal","sideproject"}},
        {"gui_process_id",QString::number(QCoreApplication::applicationPid())},{"native_delegate_verified",true}};
    QSaveFile output(expectedPath+".observed.json"); const auto bytes = QJsonDocument(report).toJson();
    if (!output.open(QIODevice::WriteOnly) || output.write(bytes)!=bytes.size() || !output.commit())
        return reject("Cannot retain the restarted GUI calendar observation.");
    qInfo() << "Restarted GUI Calendar verified one edited event, exact source fields/version and accepted self RSVP in the adapter, QML model and visible delegate.";
    return true;
}

GuiCalendarSelfTest::GuiCalendarSelfTest(MailBackend &backend, QQuickWindow *window,
    QString profile, QString engine, std::function<void(const QVariantMap &, const QString &)> finished, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_finished(std::move(finished)), m_profile(std::move(profile)), m_engine(std::move(engine)) {
    m_timer.setInterval(80);
    connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    connect(&m_backend, &MailBackend::calendarWriteFinished, this, [this](bool ok, const QVariantMap &result, const QString &error) {
        m_written = true; m_writeOk = ok; m_result = result; m_writeError = error;
    });
    m_window->setProperty("settingsOpen", false); m_window->setProperty("workspace", "calendar");
    m_backend.setCalendarDate("2026-09-11"); m_backend.setCalendarView("day");
    m_timer.start();
}
QQuickItem *GuiCalendarSelfTest::item(const QString &name) const { return findVisible(m_window->contentItem(), name); }
QObject *GuiCalendarSelfTest::popup(const QString &name) const {
    for (auto object : m_window->findChildren<QObject *>(name)) if (object->property("opened").toBool()) return object;
    return nullptr;
}
bool GuiCalendarSelfTest::click(const QString &name) {
    return clickItem(item(name));
}
bool GuiCalendarSelfTest::clickItem(QQuickItem *control) {
    if (!control || !control->isEnabled()) return false;
    reveal(control);
    const auto position = control->mapToScene({control->width() / 2, control->height() / 2});
    const QPointF global = m_window->mapToGlobal(position.toPoint());
    QMouseEvent press(QEvent::MouseButtonPress, position, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &press); QCoreApplication::sendEvent(m_window, &release);
    return true;
}
bool GuiCalendarSelfTest::type(const QString &name, const QString &text) {
    auto field = item(name);
    if (!field || !field->isEnabled()) return false;
    reveal(field); field->forceActiveFocus(); QMetaObject::invokeMethod(field, "selectAll");
    if (text.isEmpty()) key(field, Qt::Key_Backspace);
    else { QInputMethodEvent input; input.setCommitString(text); QCoreApplication::sendEvent(field, &input); }
    return field->property("text").toString() == text;
}
QVariantMap GuiCalendarSelfTest::occurrence() const {
    for (const auto &value : m_backend.events()) if (value.toMap()["event_id"].toString() == m_eventId) return value.toMap();
    return {};
}
bool GuiCalendarSelfTest::capture(const QString &name) {
    const auto directory = qEnvironmentVariable("FM_GUI_TEST_SCREENSHOTS");
    if (directory.isEmpty()) return true;
    if (!QDir::isAbsolutePath(directory) || !QDir().mkpath(directory) || !m_window->grabWindow().save(QDir(directory).filePath(name))) {
        finish("Cannot save the explicitly requested native calendar screenshot."); return false;
    }
    return true;
}
bool GuiCalendarSelfTest::startConcurrentEdit() {
    const QJsonObject fields{{"title", "CLI changed this event"}, {"start", "2026-09-11T15:30:00-04:00"}, {"end", "2026-09-11T16:00:00-04:00"},
        {"all_day", false}, {"time_zone", "America/Toronto"}, {"location", "Fixture room"}, {"description", "Human calendar edit — Montréal 日本語"},
        {"conference_url", "https://example.test/meeting"}, {"attendees", QJsonArray{"you@windscribe.example"}}};
    const QJsonObject params{{"request_id", "gui-selftest-concurrent-edit"}, {"kind", "edit"}, {"calendar_id", "primary"},
        {"event_id", m_eventId}, {"expected_etag", occurrence()["etag"].toString()}, {"fields", fields}};
    const auto path = QDir(m_profile).filePath("native-calendar-conflict.json");
    QFile file(path); const auto bytes = QJsonDocument(params).toJson();
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size()) { finish("Cannot prepare the independent calendar edit."); return false; }
    file.close();
    connect(&m_external, &QProcess::finished, this, [this](int code, QProcess::ExitStatus status) {
        const auto reply = QJsonDocument::fromJson(m_external.readAllStandardOutput()).object();
        if (code != 0 || status != QProcess::NormalExit || !reply["ok"].toBool()
            || reply["result"].toObject()["target"].toObject()["event"].toString() != m_eventId)
            m_externalError = "Independent CLI calendar edit failed: " + QString::fromUtf8(m_external.readAllStandardError());
        m_externalDone = true;
    });
    connect(&m_external, &QProcess::errorOccurred, this, [this](auto) { m_externalError = "Cannot run the independent calendar CLI."; m_externalDone = true; });
    m_external.start(QDir(QFileInfo(m_engine).absolutePath()).filePath("fm"
#ifdef Q_OS_WIN
        ".exe"
#endif
    ), {"--profile", m_profile, "--engine", m_engine, "--account", "windscribe", "--json", "calendar-write", "--params-file", path});
    return true;
}
bool GuiCalendarSelfTest::checkTimeline(const QString &date, const QString &label) {
    // The day marker names the date, weekday and Today/Tomorrow/Yesterday.
    const auto displayed = item("timelineDayMarker");
    if (m_backend.timelineDate() != date || !displayed || !unclipped(displayed) || displayed->property("label").toString() != label
        || m_backend.calendarDate() != "2026-09-14" || m_backend.calendarView() != m_timelineCalendarView) {
        finish(QString("Timeline navigation expected %1 (%2) with independent Calendar date 2026-09-14; got timeline=%3, label=%4, Calendar=%5/%6.")
            .arg(date,label,m_backend.timelineDate(),displayed ? displayed->property("label").toString() : "missing",
                m_backend.calendarDate(),m_backend.calendarView()));
        return false;
    }
    return true;
}
void GuiCalendarSelfTest::finish(const QString &error) {
    m_timer.stop(); m_backend.setProperty("selfTestExpectedError", false); auto callback = std::move(m_finished); m_finished = {}; callback(m_result, error); deleteLater();
}
void GuiCalendarSelfTest::step() {
    if (++m_ticks > 350) { finish(QString("Native calendar controls timed out at stage %1 (Calendar=%2, timeline=%3): %4")
        .arg(m_stage).arg(m_backend.calendarDate(),m_backend.timelineDate(),m_backend.error())); return; }
    if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE")) qInfo() << "Native calendar form stage" << m_stage;
    switch (m_stage) {
    case 0: {
        // The header New button is gone; Calendar's own entry point (also its n key) opens a new event.
        const auto calendar = item("calendarWorkspace");
        if (!m_backend.calendarReady() || !calendar || !QMetaObject::invokeMethod(calendar, "createEvent")) return;
        ++m_stage; break;
    }
    case 1: {
        const auto editor = popup("eventEditor"); const auto title = item("eventTitle");
        if (!editor || !title) return;
        if (!unclipped(title) || title->width() < editor->property("width").toDouble() * .75) { finish("Event Title must be initially visible and use the form width."); return; }
        if (m_backend.preferences()["default_event_duration_minutes"].toInt()!=45
            || !item("eventStartTime") || !item("eventEndTime")
            || item("eventStartTime")->property("text").toString()!="09:00"
            || item("eventEndTime")->property("text").toString()!="09:45") {
            finish("The new native event did not use the 45-minute duration saved through Settings."); return;
        }
        if (!capture("event-editor.png")) return;
        auto target = item("eventTargetCalendar");
        if (!target) { finish("The writable calendar selector is missing."); return; }
        int index = 0, selected = -1;
        for (const auto &value : m_backend.calendarSources()) if (value.toMap()["writable"].toBool()) {
            if (value.toMap()["account"].toString() == "windscribe") selected = index;
            ++index;
        }
        if (selected < 0) { finish("No writable Windscribe calendar is available."); return; }
        target->forceActiveFocus(); key(target, Qt::Key_Home);
        for (int count = 0; count < selected; ++count) key(target, Qt::Key_Down);
        if (editor->property("targetAccount").toString() != "windscribe") { finish("Native calendar selection did not preserve the explicit target account."); return; }
        if (!type("eventTitle", "") || !type("eventStartTime", "15:30") || !type("eventEndTime", "16:00")
            || !type("eventLocation", "Fixture room") || !type("eventConference", "https://example.test/meeting")
            || !type("eventAttendees", "you@windscribe.example") || !type("eventDescription", "Human calendar edit — Montréal 日本語")) { finish("Native input did not reach all event fields."); return; }
        m_written = false;
        m_backend.setProperty("selfTestExpectedError", true);
        if (!click("eventSaveButton")) { finish("The native Save event button is unavailable."); return; }
        ++m_stage; break;
    }
    case 2:
        if (!m_written) return;
        if (m_writeOk || m_result["code"].toString() != "invalid_request" || !popup("eventEditor")
            || !item("eventLocation") || item("eventLocation")->property("text").toString() != "Fixture room"
            || item("eventDescription")->property("text").toString() != "Human calendar edit — Montréal 日本語") { finish("Rejected event validation lost entered fields or closed the editor."); return; }
        if (!capture("event-editor-validation.png")) return;
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError", false);
        if (!type("eventTitle", "Native walkthrough event")) { finish("Cannot correct the retained event title."); return; }
        m_written = false; click("eventSaveButton"); ++m_stage; break;
    case 3:
        if (!m_written) return;
        if (!m_writeOk || m_result["state"].toString() != "fake_acknowledged") { finish("Native creation failed: " + m_writeError); return; }
        m_eventId = m_result["target"].toMap()["event"].toString();
        if (popup("eventEditor") || !m_backend.calendarReady() || occurrence().isEmpty()) return;
        if (!click("calendarEvent-" + m_eventId)) return;
        ++m_stage; break;
    case 4:
        if (!popup("eventDetails")) return;
        if (!capture("event-details.png") || !click("eventEditButton")) return;
        ++m_stage; break;
    case 5:
        if (!popup("eventEditor")) return;
        if (!item("eventStartTime") || !item("eventEndTime")
            || item("eventStartTime")->property("text").toString()!="15:30"
            || item("eventEndTime")->property("text").toString()!="16:00") {
            finish("The new-event duration preference rewrote an existing event's native edit fields."); return;
        }
        if (!type("eventTitle", "Edited native walkthrough event")) { finish("Native edit title input failed."); return; }
        if (!startConcurrentEdit()) return;
        m_stage = 50; break;
    case 50:
        if (!m_externalDone) return;
        if (!m_externalError.isEmpty()) { finish(m_externalError); return; }
        m_backend.setProperty("selfTestExpectedError", true); m_written = false;
        if (!click("eventSaveButton")) { finish("Cannot submit the native conflicting edit."); return; }
        m_stage = 51; break;
    case 51: {
        if (!m_written) return;
        const auto editor = popup("eventEditor"); const auto title = item("eventTitle");
        if (m_writeOk || m_result["code"].toString() != "stale_revision" || !editor || !title
            || title->property("text").toString() != "Edited native walkthrough event" || !editor->property("staleRevision").toBool()) {
            finish("A stale calendar save did not retain the user's editable fields."); return;
        }
        if (!item("eventUseCurrentVersion")) return;
        if (!capture("event-editor-stale.png")) return;
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError", false);
        if (!click("eventUseCurrentVersion")) return;
        m_stage = 52; break;
    }
    case 52:
        if (!popup("eventEditor") || popup("eventEditor")->property("staleRevision").toBool()
            || item("eventTitle")->property("text").toString() != "Edited native walkthrough event") { finish("Explicit stale recovery replaced the user's entered event fields."); return; }
        m_written = false;
        if (!click("eventSaveButton")) { finish("Cannot save the explicitly rebased native event."); return; }
        m_stage = 6; break;
    case 6:
        if (!m_written) return;
        if (!m_writeOk) { finish("Native edit failed: " + m_writeError); return; }
        if (popup("eventEditor") || !m_backend.calendarReady() || occurrence()["title"].toString() != "Edited native walkthrough event") return;
        if (!click("calendarEvent-" + m_eventId)) return;
        ++m_stage; break;
    case 7:
        if (!popup("eventDetails")) return;
        m_written = false;
        if (!click("eventRsvp-accepted")) return;
        ++m_stage; break;
    case 8:
        if (!m_written) return;
        if (!m_writeOk) { finish("Native RSVP failed: " + m_writeError); return; }
        if (!m_backend.calendarReady() || occurrence()["self_response"].toString() != "accepted") return;
        if (!capture("event-details-rsvp.png")) return;
        if (!click("eventViewButton")) return;
        m_stage = 80; break;
    case 80: {
        // View replaces the card with the reminder screen for the same event:
        // no Snooze, and Close instead of Dismiss.
        const auto view = popup("eventFullView"); const auto close = item("reminderDismiss");
        if (!view || !close) return;
        const auto content = qobject_cast<QQuickItem *>(view->property("contentItem").value<QObject *>());
        if (popup("eventDetails") || item("reminderSnooze") || close->property("text").toString() != "Close"
            || !content || !visibleTitle(content, "Edited native walkthrough event")) {
            finish("View did not replace the card with a Close-only full view of the same event."); return;
        }
        if (!capture("event-view.png") || !click("reminderDismiss")) return;
        m_stage = 81; break;
    }
    case 81:
        if (popup("eventFullView")) return;
        m_backend.setCalendarView("week"); m_stage = 82; break;
    case 82: {
        // Mail's Next row: from any view, Day view on the event's day with its card open.
        const auto calendar = item("calendarWorkspace"); const auto event = occurrence();
        if (!m_backend.calendarReady() || m_backend.calendarView() != "week" || !calendar || event.isEmpty()) return;
        // An invitation email passes Core's row, which has no layout date.
        auto undated = event; undated.remove("date");
        if (m_backend.calendarDayOf(event["start"].toString()) != event["date"].toString()) { finish("calendarDayOf does not match the Calendar layout's day."); return; }
        if (!QMetaObject::invokeMethod(calendar, "showEventInDay", Q_ARG(QVariant, QVariant(undated)))) { finish("Calendar has no showEventInDay entry point."); return; }
        m_stage = 83; break;
    }
    case 83: {
        const auto details = popup("eventDetails");
        if (!details) return;
        const auto shown = variantValue(details->property("eventData")).toMap();
        if (m_backend.calendarView() != "day" || m_backend.calendarDate() != "2026-09-11" || details->property("timelineSource").toBool()
            || shown["event_id"].toString() != m_eventId || !item("calendarEvent-" + m_eventId)) {
            finish("showEventInDay did not open the event's card on its Day view."); return;
        }
        if (!capture("event-day-details.png")) return;
        QMetaObject::invokeMethod(details, "close");
        m_backend.setCalendarView("week"); m_stage = 9; break;
    }
    case 9: {
        static const QStringList views{"week", "month", "agenda", "day"};
        if (!m_backend.calendarReady() || m_backend.calendarView() != views[m_view]) return;
        const auto event = occurrence();
        if (event["title"].toString() != "Edited native walkthrough event" || event["self_response"].toString() != "accepted"
            || event["account"].toString() != "windscribe" || event["location"].toString() != "Fixture room") { finish("Native calendar views disagree on the created event's identity or fields."); return; }
        if (views[m_view] == "agenda") {
            const auto calendar = item("calendarWorkspace");
            if (!calendar) { finish("The native agenda surface is missing."); return; }
            const auto value = calendar->property("agendaEvents");
            const auto rows = value.canConvert<QJSValue>() ? value.value<QJSValue>().toVariant().toList() : value.toList();
            int fridayTimed = -1, saturdayAllDay = -1;
            for (qsizetype index = 0; index < rows.size(); ++index) {
                const auto row = rows[index].toMap();
                if (row["event_id"].toString() == m_eventId) fridayTimed = int(index);
                if (row["all_day"].toBool() && row["date"].toString() == "2026-09-12") saturdayAllDay = int(index);
            }
            if (fridayTimed < 0 || saturdayAllDay <= fridayTimed) { finish("Agenda placed Saturday's all-day event before Friday's timed event."); return; }
        }
        if (++m_view == views.size()) { m_stage = 10; break; }
        m_backend.setCalendarView(views[m_view]); break;
    }
    case 10:
        if (!m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferencesPending()) return;
        if (m_backend.todayDate() != "2026-09-11" || !QDate::fromString(m_backend.timelineDate(),Qt::ISODate).isValid()) {
            finish("Native timeline controls require the fixed 2026-09-11 fixture clock and a valid starting date."); return;
        }
        m_originalTimelineDate = m_backend.timelineDate(); m_timelineCalendarDate = m_backend.calendarDate();
        for (const auto &name : {"calendar_date", "timeline_date"}) m_timelineDatePreferences[name] = m_backend.preferences().value(name);
        m_timelineCalendarView = m_backend.calendarView(); m_timelineWorkspace = m_window->property("workspace").toString();
        m_timelineSettingsOpen = m_window->property("settingsOpen").toBool(); m_timelineFocus = m_window->activeFocusItem();
        // Establish distinct dates before exercising actual buttons. The native
        // Today control below must restore the fixture clock, not Calendar's date.
        m_backend.setCalendarDate("2026-09-14"); m_backend.showTimelineToday();
        m_window->setProperty("settingsOpen",false); m_window->setProperty("workspace","mail");
        m_stage = 11; break;
    case 11:
        if (!m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferencesPending()) return;
        if (!checkTimeline("2026-09-11","11 Fri · Today")) return;
        m_backend.moveTimeline(-1);
        m_stage = 12; break;
    case 12:
        if (!m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferencesPending()) return;
        if (!checkTimeline("2026-09-10","10 Thu · Yesterday")) return;
        m_backend.moveTimeline(1);
        m_stage = 13; break;
    case 13:
        if (!m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferencesPending()) return;
        if (!checkTimeline("2026-09-11","11 Fri · Today")) return;
        m_backend.moveTimeline(1);
        m_stage = 14; break;
    case 14:
        if (!m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferencesPending()) return;
        if (!checkTimeline("2026-09-12","12 Sat · Tomorrow")) return;
        if (!click("timelineToday")) { finish("The native timeline Back to today button is unavailable away from today."); return; }
        m_stage = 15; break;
    case 15:
        if (!m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferencesPending()) return;
        if (!checkTimeline("2026-09-11","11 Fri · Today")) return;
        if (item("timelineToday")) { finish("Back to today remained visible after returning to the fixture date."); return; }
        m_timelineSurface=headerTimeline(m_window->contentItem());
        if (!m_timelineSurface || m_backend.preferences()["timeline_position"].toString()!="header") {
            finish("The saved Header timeline preference did not produce a visible native header strip."); return;
        }
        if (auto block=editableTimelineBlock(m_timelineSurface)) {
            m_timelineEvent=variantValue(block->property("modelData")).toMap();
            if (!clickItem(block)) return;
        } else { finish("No permitted native timeline event is accessible for the header popover walkthrough."); return; }
        m_stage=17; break;
    case 17: {
        const auto details=popup("eventDetails");
        if (!details) return;
        const auto bottom=m_timelineSurface->mapToScene({0,m_timelineSurface->height()}).y();
        const auto data=variantValue(details->property("eventData")).toMap();
        if (!details->property("timelineSource").toBool() || details->property("y").toDouble()<bottom
            || details->property("y").toDouble()+details->property("height").toDouble()>m_window->height()
            || data["event_id"]!=m_timelineEvent["event_id"] || data["account"]!=m_timelineEvent["account"]
            || data["calendar_id"]!=m_timelineEvent["calendar_id"]) {
            finish("Header timeline details overlap the strip, leave the window or change source identity."); return;
        }
        if (!capture("timeline-header-details.png") || !click("eventEditButton")) return;
        m_stage=18; break;
    }
    case 18: {
        const auto editor=popup("eventEditor");
        if (!editor) return;
        const auto bottom=m_timelineSurface->mapToScene({0,m_timelineSurface->height()}).y();
        if (!editor->property("timelineAtTop").toBool() || editor->property("y").toDouble()<bottom
            || editor->property("y").toDouble()+editor->property("height").toDouble()>m_window->height()+1
            || !item("eventTitle") || item("eventTitle")->property("text").toString()!=m_timelineEvent["title"].toString()) {
            finish("Header timeline Edit did not retain readable source fields below the strip."); return;
        }
        if (!capture("timeline-header-editor.png") || !click("eventCancelButton")) return;
        m_stage=19; break;
    }
    case 19:
        if (popup("eventEditor") || popup("eventDetails")) return;
        m_backend.setCalendarDate(m_timelineCalendarDate);
        m_backend.moveTimeline(int(QDate::fromString(m_backend.timelineDate(),Qt::ISODate)
            .daysTo(QDate::fromString(m_originalTimelineDate,Qt::ISODate))));
        // Preserve null (follow today) as well as explicitly browsed dates.
        m_backend.setPreferences(m_timelineDatePreferences);
        m_window->setProperty("workspace",m_timelineWorkspace); m_window->setProperty("settingsOpen",m_timelineSettingsOpen);
        m_stage = 16; break;
    case 16:
        if (!m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferencesPending()) return;
        if (m_backend.calendarDate() != m_timelineCalendarDate || m_backend.calendarView() != m_timelineCalendarView
            || m_backend.timelineDate() != m_originalTimelineDate || m_window->property("workspace").toString() != m_timelineWorkspace
            || m_window->property("settingsOpen").toBool() != m_timelineSettingsOpen) {
            finish("Timeline control cleanup lost the original Calendar, timeline or workspace state."); return;
        }
        for (auto it = m_timelineDatePreferences.cbegin(); it != m_timelineDatePreferences.cend(); ++it)
            if (m_backend.preferences().value(it.key()) != it.value()) { finish("Timeline navigation changed the original saved date preference."); return; }
        if (m_timelineFocus && m_timelineFocus->isVisible() && m_timelineFocus->isEnabled()) m_timelineFocus->forceActiveFocus();
        qInfo() << "Native timeline passed: Previous/Next/Today preserve the independent Calendar date; Header details/editor retain exact permitted source below the strip; 45-minute create default leaves existing edit duration intact; starting state restored.";
        finish(); break;
    }
}
