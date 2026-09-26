// CalendarView UI test for delete and decline with Undo, on a real MailBackend
// over the scripted engine: the six-second hold, which events it hides, Activity
// recovery, and cancellation on close or when the source or permission changes.
#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "CalendarLayout.h"
#include "ReplyDocument.h"
#include <QFontDatabase>
#include <QGuiApplication>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
using namespace GoogleOperationFixture;
namespace {
QQuickItem *find(QQuickItem *item, const QString &name) {
    if (item->objectName() == name) return item;
    for (auto child : item->childItems()) if (auto found = find(child, name)) return found;
    return nullptr;
}
QVariantMap event(const QString &id, const QString &account = "fixture-account") {
    return {{"id", account + ":calendar:" + id}, {"account", account}, {"account_name", "Personal"}, {"calendar_id", "calendar"}, {"calendar_name", "Calendar"},
        {"event_id", id}, {"etag", "v1"}, {"title", "Review " + id}, {"all_day", false}, {"busy_only", false}, {"can_edit", true}, {"can_delete", true},
        {"start", "2026-09-15T13:00:00Z"}, {"end", "2026-09-15T14:00:00Z"}, {"time_zone", "America/Toronto"}, {"attendees", QVariantList{}},
        {"occurrence_key", QVariantMap{{"account", account}, {"calendar", "calendar"}, {"series", id}, {"original_start", QVariant()}}}};
}
QString encoded(const QVariantMap &value) { return QString::fromUtf8(QJsonDocument(QJsonObject::fromVariantMap(value)).toJson(QJsonDocument::Compact)); }
}
class CalendarUndoTest {
public:
static void run() {
    for (const auto weight : {"Regular", "Medium", "SemiBold"}) require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf") >= 0, "bundled font");
    QGuiApplication::setFont(QFont("Geist", 10));
    Engine fixture; MailBackend backend(fixture.profile, "unused-fixture-engine", nullptr, false, true);
    backend.m_connected = true; backend.m_syntheticProfile = false; backend.m_account = "fixture-account"; backend.m_enginePid = QCoreApplication::applicationPid();
    backend.m_preferences = {{"calendar_display_time_zone", "America/Toronto"}, {"week_start", "monday"}};
    backend.m_calendarDate = "2026-09-15"; backend.m_timelineDate = "2026-09-15"; backend.m_calendarView = "day"; backend.m_calendarReady = true; backend.m_timelineReady = true;
    QVariantMap account{{"id", "fixture-account"}, {"name", "Personal"}, {"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}},
        {"google", QVariantMap{{"capabilities", QVariantMap{{"calendar_read", true}, {"calendar_write", true}}}}}};
    backend.m_allAccounts = {account}; backend.publishAccounts({account}); backend.m_googleSync = {{"fixture-account", QVariantMap{{"active", true}}}};
    backend.m_calendarSources = {QVariantMap{{"account", "fixture-account"}, {"calendar_id", "calendar"}, {"writable", true}, {"can_create", true}, {"permission", "owner"}, {"time_zone", "America/Toronto"}}};
    QJsonArray raw{QJsonObject::fromVariantMap(event("a")), QJsonObject::fromVariantMap(event("b")), QJsonObject::fromVariantMap(event("a", "other-account"))};
    auto publish = [&] { backend.m_events = calendarPresentation(raw, QDate(2026,9,15), 7, QTimeZone("America/Toronto")); backend.m_todayEvents = backend.m_events; backend.m_calendarSearchResult["events"] = backend.m_events; emit backend.eventsChanged(); emit backend.timelineChanged(); };
    publish();
    backend.m_reminders = backend.m_events;
    auto row = [&](const QString &id) { for (const auto &v : backend.m_events) if (v.toMap()["event_id"] == id && v.toMap()["account"] == "fixture-account") return v.toMap(); return QVariantMap{}; };
    QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr); backend.m_socket.connectToServer(fixture.server.serverName());
    await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached connection");
    bool hello = false; backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; }); await([&] { return hello; }, "cached authentication");
    QList<QJsonObject> writes; QJsonObject lastOperation; qint64 dispatchedAt = 0; QString outcome = "acknowledged"; int reconciliations = 0;
    fixture.customRequest = [&](QLocalSocket *peer, const QJsonObject &request) {
        const auto method = request["method"].toString(), id = request["id"].toString(); const auto p = request["params"].toObject();
        if (method == "google_calendar_write") {
            writes.append(request); dispatchedAt = backend.m_actionClock.elapsed();
            lastOperation = {{"request_id", p["request_id"]}, {"kind", "delete"}, {"state", outcome}, {"revision", 2}, {"account", request["account"]},
                {"target", QJsonObject{{"account", request["account"]}, {"calendar", p["calendar_id"]}, {"event_id", p["event_id"]}}}};
            if (outcome == "acknowledged") for (int i = int(raw.size()) - 1; i >= 0; --i) if (raw[i].toObject()["event_id"] == p["event_id"] && raw[i].toObject()["account"] == request["account"]) raw.removeAt(i);
            peer->write(reply(id, lastOperation)); return true;
        }
        if (method == "google_calendar_reconcile") { require(p["request_id"] == lastOperation["request_id"], "recovery retains exact request"); ++reconciliations; lastOperation["state"] = "reconciled"; peer->write(reply(id, lastOperation)); return true; }
        if (method == "calendar_combined") peer->write(reply(id, {{"events", raw}, {"from", p["from"]}, {"to", p["to"]}, {"time_zone", "America/Toronto"}, {"revision", 1}, {"coverage", QJsonObject{{"selected_calendars", 1}, {"complete", true}}}}));
        else if (method == "calendar_sources") peer->write(reply(id, {{"calendars", QJsonArray::fromVariantList(backend.m_calendarSources)}}));
        else if (method == "calendar_operations") peer->write(reply(id, {{"operations", lastOperation.isEmpty() ? QJsonArray{} : QJsonArray{lastOperation}}}));
        else if (method == "preferences_get") peer->write(reply(id, {{"revision", 1}, {"preferences", QJsonObject::fromVariantMap(backend.m_preferences)}}));
        else peer->write(reply(id, {})); return true;
    };
    QTemporaryDir copy; const auto path = copy.filePath("CalendarUndo"); require(QDir().mkpath(path), "module directory"); QByteArray registration("module CalendarUndo\nsingleton Theme 1.0 Theme.qml\n");
    for (const auto &name : QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"}, QDir::Files)) {
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + name, path + '/' + name), "production QML copy");
        if (name == "ShortcutRegistry.qml") registration += "singleton ";
        if (name != "Theme.qml") registration += name.chopped(4).toUtf8() + " 1.0 " + name.toUtf8() + '\n';
    }
    QFile module(path + "/qmldir"); require(module.open(QIODevice::WriteOnly), "module registration"); module.write(registration); module.close();
    qmlRegisterType<ReplyDocument>("CalendarUndo", 1, 0, "ReplyDocument"); QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings = false;
    QObject::connect(&qml, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { warnings = true; for (const auto &e : errors) qWarning() << e; });
    qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
    qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import CalendarUndo 1.0
ApplicationWindow { width:1100;height:800;visible:true;color:Theme.bg
 property bool singleKeysAllowed:true;property bool searchPopupOpen:false
 function appearance(theme){Theme.themeId=theme}
 CalendarView {anchors.fill:parent}
 ActionToast {anchors.horizontalCenter:parent.horizontalCenter;anchors.bottom:parent.bottom;anchors.bottomMargin:16}
 CalendarOperations {id:activity;objectName:"toastActivity"}
 Connections {target:backend;function onCalendarActivityRequested(){activity.open()}}
})QML", QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(), "production Calendar loads"); auto window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); require(window, "window");
    await([&] { return window->isExposed(); }, "Calendar exposed"); auto details = window->findChild<QObject *>("eventDetails"); require(details, "event details");
    auto click = [&](const QString &name) { auto item = find(window->contentItem(), name); require(item && item->isVisible() && item->isEnabled(), "visible enabled Calendar control"); require(QMetaObject::invokeMethod(item, "clicked"), "real control handler"); };
    auto open = [&](const QVariantMap &value) { require(QMetaObject::invokeMethod(details, "openAt", Q_ARG(QVariant, value), Q_ARG(QVariant, QVariant{})), "open actual event details"); await([&] { return details->property("opened").toBool(); }, "details open"); };
    open(row("a")); click("eventScopedCancel"); auto confirmation = window->findChild<QObject *>("eventDetailsDeleteConfirmation"); require(confirmation, "common confirmation");
    await([&] { return confirmation->property("opened").toBool(); }, "confirmation opens"); require(writes.isEmpty(), "no write before confirmation"); click("confirmDetailsDelete");
    await([&] { return !details->property("visible").toBool(); }, "queued deletion closes details");
    require(writes.isEmpty() && backend.events().size() == 2 && backend.todayEvents().size() == 2 && backend.calendarSearch()["events"].toList().size() == 2, "pending deletion hidden across Calendar timeline and search without provider write");
    require(backend.m_events.size() == 3 && backend.events().first().toMap()["columns"] == 2, "cache retained and remaining overlap fills space");
    click("actionToastButton"); require(backend.events().size() == 3 && writes.isEmpty(), "Undo restores current layout without Google action");
    require(backend.reminders().size() == 3, "Undo preserves existing reminder presentations");
    auto params = [&](const QVariantMap &e, const QString &id) { return QVariantMap{{"account", e["account"]}, {"calendar_id", e["calendar_id"]}, {"event_id", e["event_id"]}, {"expected_etag", e["etag"]}, {"kind", "delete"}, {"request_id", id}}; };
    // A real six-second interval is measured on the same production queue.
    const auto single = row("a"), request = params(single, "timed-delete"); require(backend.queueCalendarDeletion(request, single).isEmpty(), "queue exact deletion");
    const auto deadline = backend.m_calendarDeletions.cbegin()->deadline;
    const auto captures = qEnvironmentVariable("FM_CALENDAR_UNDO_SCREENSHOTS");
    if (!captures.isEmpty()) { QDir().mkpath(captures); for (const auto theme : {"graphite", "paper", "terminal"}) { QMetaObject::invokeMethod(window, "appearance", Q_ARG(QVariant, QString(theme))); QCoreApplication::processEvents(); require(window->grabWindow().save(captures + "/calendar-undo-" + theme + ".png"), "theme capture"); } }
    QElapsedTimer wait; wait.start(); while (writes.isEmpty() && wait.elapsed() < 6500) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    require(writes.size() == 1 && dispatchedAt >= deadline, "provider command starts only after real six seconds");
    auto expected = QJsonObject::fromVariantMap(request); expected.remove("account"); require(writes[0]["params"].toObject() == expected && writes[0]["account"] == request["account"].toString(), "original immutable request dispatched once");
    await([&] { return backend.m_calendarWrites == 0 && backend.m_inFlight.isEmpty(); }, "accepted deletion readback"); require(backend.events().size() == 2, "accepted cached deletion remains absent");
    // Unknown results restore the cached row and use the existing activity recovery.
    backend.m_calendarUndoMs = 60; outcome = "unknown"; const auto uncertain = row("b");
    require(backend.queueCalendarDeletion(params(uncertain, "unknown-delete"), uncertain).isEmpty(), "queue uncertain fixture");
    await([&] { return writes.size() == 2 && backend.m_calendarWrites == 0 && backend.actionToast()["action"] == "calendar_activity"; }, "unknown outcome exposed after dispatch");
    require(backend.events().size() == 2, "unknown outcome restores cached event"); click("actionToastButton"); auto activity = window->findChild<QObject *>("toastActivity");
    await([&] { return activity->property("opened").toBool() && !backend.m_calendarOperationsLoading; }, "global Calendar activity opens");
    click("calendarOperationResolve-unknown-delete"); await([&] { return reconciliations == 1 && backend.m_calendarWrites == 0; }, "read recovery uses exact original deletion");
    QMetaObject::invokeMethod(activity, "close"); await([&] { return !activity->property("visible").toBool(); }, "activity closes");
    backend.m_calendarUndoMs = 6000;
    auto recurring = event("series"); recurring["id"] = "series-occurrence"; recurring["can_delete"] = false; recurring["can_resolve_occurrence"] = true;
    const QVariantMap original{{"kind", "zoned"}, {"instant", "2026-09-15T13:00:00Z"}, {"time_zone", "America/Toronto"}};
    recurring["recurring_selector"] = QVariantMap{{"series_id", "series"}, {"original_start", original}};
    auto later = recurring; later["id"] = "later-occurrence"; later["start"] = "2026-09-16T13:00:00Z"; later["end"] = "2026-09-16T14:00:00Z";
    auto nextOriginal = original; nextOriginal["instant"] = "2026-09-16T13:00:00Z"; later["recurring_selector"] = QVariantMap{{"series_id", "series"}, {"original_start", nextOriginal}};
    raw = {QJsonObject::fromVariantMap(recurring), QJsonObject::fromVariantMap(later), QJsonObject::fromVariantMap(event("series", "other-account"))}; publish();
    auto resolved = row("series"); const auto source = resolved; resolved["event_id"] = "instance"; resolved["etag"] = "instance-v1"; resolved["can_delete"] = true;
    resolved["occurrence_ui_context"] = QVariantMap{{"source", encoded(source)}, {"authority", encoded(backend.calendarOccurrenceContext("fixture-account", "calendar"))}};
    for (const auto scope : {"occurrence", "following", "series"}) {
        auto selection = resolved; QVariantMap context{{"scope", scope}, {"series_id", "series"}, {"expected_master_etag", "v1"}};
        if (QString(scope) != "series") context["original_start"] = original;
        else { selection["event_id"] = "series"; selection["etag"] = "v1"; }
        selection["recurring_context"] = context; auto value = params(selection, QString("scope-") + scope);
        if (QString(scope) == "following") context["reset_future_exceptions"] = true;
        value["recurring"] = context; require(backend.queueCalendarDeletion(value, selection).isEmpty(), "resolved recurring deletion queues");
        require(!backend.queueCalendarDeletion(value, selection).isEmpty(), "duplicate scope cannot create another pending deletion");
        require(backend.events().size() == (QString(scope) == "occurrence" ? 2 : 1), "scope hides exactly selected occurrences and preserves another account");
        backend.activateActionToast(); require(backend.events().size() == 3 && writes.size() == 2, "recurring Undo preserves provider and other occurrences");
    }
    auto dated = event("dates"); dated["id"] = "dated-current"; dated["all_day"] = true; dated["start"] = "2026-09-15"; dated["end"] = "2026-09-17"; dated["can_delete"] = false;
    const QVariantMap datedOriginal{{"kind", "date"}, {"date", "2026-09-15"}};
    dated["recurring_selector"] = QVariantMap{{"series_id", "dates"}, {"original_start", datedOriginal}};
    auto earlierMoved = dated; earlierMoved["id"] = "dated-earlier-moved"; earlierMoved["start"] = "2026-09-19"; earlierMoved["end"] = "2026-09-20"; earlierMoved.remove("recurring_selector");
    earlierMoved["occurrence_key"] = QVariantMap{{"series", "dates"}, {"original_start", "2026-09-14"}};
    auto laterMoved = earlierMoved; laterMoved["id"] = "dated-later-moved"; laterMoved["start"] = "2026-09-18"; laterMoved["end"] = "2026-09-19";
    laterMoved["occurrence_key"] = QVariantMap{{"series", "dates"}, {"original_start", "2026-09-16"}};
    raw = {QJsonObject::fromVariantMap(dated), QJsonObject::fromVariantMap(earlierMoved), QJsonObject::fromVariantMap(laterMoved)}; publish();
    auto datedResolved = row("dates"); const auto datedSource = datedResolved; datedResolved["can_delete"] = true;
    QVariantMap datedContext{{"scope", "following"}, {"series_id", "dates"}, {"expected_master_etag", "v1"}, {"original_start", datedOriginal}};
    datedResolved["recurring_context"] = datedContext; datedResolved["occurrence_ui_context"] = QVariantMap{{"source", encoded(datedSource)}, {"authority", encoded(backend.calendarOccurrenceContext("fixture-account", "calendar"))}};
    auto datedRequest = params(datedResolved, "dated-following"); datedContext["reset_future_exceptions"] = true; datedRequest["recurring"] = datedContext;
    require(backend.queueCalendarDeletion(datedRequest, datedResolved).isEmpty(), "all-day following deletion queues");
    require(backend.events().size() == 1 && backend.events()[0].toMap()["id"] == "dated-earlier-moved", "following uses original date and keeps earlier moved exception");
    backend.activateActionToast(); require(backend.events().size() == 3, "all-day spans restored by Undo");
    // Fresh-source and authority changes cancel before dispatch; navigation alone does not.
    raw = {QJsonObject::fromVariantMap(event("final"))}; publish(); const auto final = row("final");
    require(backend.queueCalendarDeletion(params(final, "authority-change"), final).isEmpty(), "queue before authority change"); ++backend.m_calendarAuthorityEpoch; backend.advanceCalendarDeletions(); require(backend.m_calendarDeletions.isEmpty() && writes.size() == 2, "authority change cancels unsent deletion");
    require(backend.queueCalendarDeletion(params(final, "source-change"), final).isEmpty(), "queue before source change"); auto changed = final; changed["etag"] = "v2"; backend.m_events = {changed}; backend.advanceCalendarDeletions(); require(backend.m_calendarDeletions.isEmpty(), "new source revision cancels unsent deletion"); publish();
    require(backend.queueCalendarDeletion(params(final, "disconnect-delete"), final).isEmpty(), "queue before disconnect"); backend.m_socket.abort();
    await([&] { return !backend.m_connected && backend.m_calendarDeletions.isEmpty(); }, "actual socket disconnect cancels unsent deletion"); require(writes.size() == 2, "disconnect makes no provider deletion");
    backend.m_socket.connectToServer(fixture.server.serverName()); await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "fixture reconnect");
    hello = false; backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; }); await([&] { return hello; }, "reconnect authentication");
    backend.m_connected = true; backend.m_googleSync = {{"fixture-account", QVariantMap{{"active", true}}}}; publish();
    require(backend.queueCalendarDeletion(params(final, "close-delete"), final).isEmpty(), "queue before close"); backend.shutdown();
    await([&] { return backend.m_shutdownEmitted; }, "clean close completes"); require(backend.m_calendarDeletions.isEmpty() && writes.size() == 2 && backend.events().size() == 1, "close cancels deletion and restores cache");
    require(!warnings, "production Calendar QML has no warnings"); qInfo() << "PASS: native Calendar confirmation and Undo, exact six-second dispatch, scope/account/layout filtering, activity recovery and close/source/authority cancellation";
}
};
int main(int argc, char **argv) { QGuiApplication app(argc, argv); QQuickStyle::setStyle("Basic"); CalendarUndoTest::run(); return 0; }
