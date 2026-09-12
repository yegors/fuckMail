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
    auto control = item(name);
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
void GuiCalendarSelfTest::finish(const QString &error) {
    m_timer.stop(); m_backend.setProperty("selfTestExpectedError", false); auto callback = std::move(m_finished); m_finished = {}; callback(m_result, error); deleteLater();
}
void GuiCalendarSelfTest::step() {
    if (++m_ticks > 350) { finish(QString("Native calendar controls timed out at stage %1: %2").arg(m_stage).arg(m_backend.error())); return; }
    if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE")) qInfo() << "Native calendar form stage" << m_stage;
    switch (m_stage) {
    case 0:
        if (!m_backend.calendarReady() || !click("newEventButton")) return;
        ++m_stage; break;
    case 1: {
        const auto editor = popup("eventEditor"); const auto title = item("eventTitle");
        if (!editor || !title) return;
        if (!unclipped(title) || title->width() < editor->property("width").toDouble() * .75) { finish("Event Title must be initially visible and use the form width."); return; }
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
        if (auto details = popup("eventDetails")) QMetaObject::invokeMethod(details, "close");
        m_backend.setCalendarView("week"); ++m_stage; break;
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
        if (++m_view == views.size()) { finish(); return; }
        m_backend.setCalendarView(views[m_view]); break;
    }
    }
}
