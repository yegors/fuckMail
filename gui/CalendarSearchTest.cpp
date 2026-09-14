#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include "HatchItem.h"
#include <QGuiApplication>
#include <QJsonArray>
#include <QFontDatabase>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>

using namespace GoogleOperationFixture;
namespace {
QQuickItem *visual(QQuickItem *root, const QString &name) {
    if (root->objectName() == name) return root;
    for (auto *child : root->childItems()) if (auto *found = visual(child, name)) return found;
    return nullptr;
}
QObject *named(QQuickWindow *root, const QString &name) {
    if (auto *found = root->findChild<QObject *>(name)) return found;
    return visual(root->contentItem(), name);
}
QJsonObject event(const QString &account = "personal", const QString &title = "Project review") {
    return {{"id", "same-occurrence"}, {"account", account}, {"account_name", account},
        {"calendar_id", "same-calendar"}, {"calendar_name", "Saved team calendar"},
        {"event_id", "instance"}, {"etag", "v1"}, {"occurrence_key", "2026-09-14T09:00:00Z"},
        {"title", title}, {"start", "2026-09-14T09:00:00Z"}, {"end", "2026-09-14T09:30:00Z"},
        {"all_day", false}, {"organizer", "organizer@example.test"}, {"organizer_name", "Organizer"},
        {"attendees", QJsonArray{}}, {"permission", "reader"}, {"can_edit", false}, {"can_delete", false}, {"can_rsvp", false}};
}
}

class CalendarSearchTest {
    struct Held { QPointer<QLocalSocket> peer; QJsonObject request; };
    struct Harness {
        Engine engine;
        MailBackend backend{engine.profile, "unused", nullptr, false, true};
        QList<Held> requests;
        QStringList methods;
        QJsonArray current{event(), event("workspace")};
        bool holdWindow = false;
        Harness() {
            backend.m_account = "personal"; backend.m_enginePid = QCoreApplication::applicationPid();
            backend.m_calendarDate = "2026-09-14"; backend.m_timelineDate = "2026-09-14";
            backend.m_preferences = {{"notifications", "off"}, {"theme", "graphite"}, {"text_size", "default"},
                {"calendar_display_time_zone", "UTC"}, {"selected_calendars", QVariant()}, {"last_workspace", "mail"}, {"startup_view", "mail"}};
            backend.m_preferencesSaving = false;
            for (const auto &id : {"personal", "workspace"}) {
                backend.m_allAccounts.append(QVariantMap{{"id", id}, {"name", id}, {"email", QString(id) + "@example.test"},
                    {"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}},
                    {"google", QVariantMap{{"connection_state", "connected"}, {"capabilities", QVariantMap{{"calendar_read", true}}}}}});
                backend.m_calendarSources.append(QVariantMap{{"account", id}, {"calendar_id", "same-calendar"},
                    {"name", "Saved team calendar"}, {"revision", 1}, {"permission", "reader"}, {"writable", false}});
            }
            require(backend.publishAccounts(backend.m_allAccounts), "stable account model");
            engine.customRequest = [this](auto *peer, const auto &request) {
                const auto method = request["method"].toString(); methods.append(method);
                if (method == "calendar_search" || (method == "calendar_combined" && holdWindow)) requests.append({peer, request});
                else {
                    QJsonObject result;
                    if (method == "calendar_combined") result = response(request, current);
                    else if (method == "preferences_save") result = {{"revision", 2}};
                    else if (method == "calendar_sources") result = {{"calendars", QJsonArray::fromVariantList(backend.m_calendarSources)}};
                    else if (method == "accounts") result = {{"accounts", QJsonArray::fromVariantList(backend.m_allAccounts)}};
                    else if (method == "view_get") result = {{"revision", 0}, {"view", QJsonObject{}}};
                    else if (method == "view_save") result = {{"revision", 1}};
                    else if (method == "conversations") result = {{"conversations", QJsonArray{}}, {"revision", 1}};
                    else if (method == "categories") result = {{"categories", QJsonArray{}}};
                    else require(false, qPrintable("unexpected cached test method: " + method));
                    peer->write(reply(request["id"].toString(), result));
                }
                return true;
            };
            QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
            backend.m_socket.connectToServer(engine.server.serverName());
            await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "private cached socket");
            bool hello = false;
            backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
            await([&] { return hello; }, "authenticated cached framing");
            backend.m_connected = true; backend.m_initialAccountsLoaded = true; backend.m_initialPreferencesLoaded = true;
        }
        ~Harness() { backend.m_shutdown = true; backend.m_calendarSearchTimer.stop(); backend.m_connected = false; backend.m_socket.abort(); }
        static QJsonObject response(const QJsonObject &request, const QJsonArray &events) {
            const auto params = request["params"].toObject();
            QJsonObject result{{"from", params["from"]}, {"to", params["to"]}, {"time_zone", "UTC"}, {"revision", 9},
                {"events", events}, {"coverage", QJsonObject{{"complete", false}, {"state", "partial"}, {"selected_calendars", 2}}}};
            if (request["method"] == "calendar_search") {
                result["query"] = params["query"]; result["limit"] = 100; result["total"] = events.size();
                result["has_more"] = false; result["continuation"] = QJsonValue::Null;
            }
            return result;
        }
        Held take(const QString &method) {
            await([&] { return !requests.isEmpty(); }, "cached search request arrives");
            const auto held = requests.takeFirst();
            require(held.request["method"] == method && !held.request.contains("account"), "shared global cached method, no mailbox retargeting");
            return held;
        }
        void complete(const Held &held, const QJsonObject &result) {
            const auto id = held.request["id"].toString(); held.peer->write(reply(id, result));
            await([&] { return !backend.m_pending.contains(id); }, "held cached reply completes");
        }
        Held search(const QString &query = "project") {
            backend.setCalendarSearchContext(true, true); backend.setCalendarSearchQuery(query);
            return take("calendar_search");
        }
    };
    static void contexts() {
        for (int kind = 0; kind < 7; ++kind) {
            Harness h; auto old = h.search();
            if (kind == 0) h.backend.setCalendarSearchQuery("new words");
            if (kind == 1) { h.backend.setCalendarSearchContext(false, true); h.backend.setCalendarSearchContext(true, true); }
            if (kind == 2) { h.backend.m_account = "workspace"; emit h.backend.accountChanged(); h.backend.m_account = "personal"; emit h.backend.accountChanged(); }
            if (kind == 3) { auto row = h.backend.m_allAccounts[0].toMap(); row["lifecycle"] = QVariantMap{{"state", "active"}, {"revision", 3}}; h.backend.m_allAccounts[0] = row; emit h.backend.accountsChanged(); }
            if (kind == 4) { h.backend.m_calendarDate = "2026-09-21"; emit h.backend.eventsChanged(); }
            if (kind == 5) { h.backend.m_preferences["selected_calendars"] = QVariantList{}; emit h.backend.preferencesChanged(); }
            if (kind == 6) { auto source = h.backend.m_calendarSources[0].toMap(); source["revision"] = 2; h.backend.m_calendarSources[0] = source; emit h.backend.eventsChanged(); }
            h.complete(old, Harness::response(old.request, {event("personal", "Obsolete response")}));
            require(h.backend.calendarSearch().value("events").toList().isEmpty(), "old query, popup epoch, account, lifecycle, date, visibility and catalog replies are refused");
            auto fresh = h.take("calendar_search");
            h.complete(fresh, Harness::response(fresh.request, {}));
            require(!h.backend.calendarSearch().value("busy").toBool(), "replacement search completes");
        }
        Harness idle;
        const auto generation = idle.backend.m_calendarSearchGeneration;
        emit idle.backend.accountsChanged(); emit idle.backend.eventsChanged();
        require(idle.backend.m_calendarSearchGeneration == generation && idle.backend.m_calendarSearchContext.isEmpty(), "inactive Mail navigation does not materialize Calendar search context");
        auto request = idle.search(); auto noZone = Harness::response(request.request, {});
        noZone["time_zone"] = QJsonValue::Null; noZone["coverage"] = QJsonObject{{"state", "display_zone_required"}};
        idle.complete(request, noZone);
        require(idle.backend.calendarSearch().value("status").toString().contains("Choose a display time zone"), "display-zone setup is explained truthfully");
    }
    static void selection() {
        Harness h; h.backend.setCalendarSearchContext(true, false); h.backend.setCalendarSearchQuery("review");
        auto search = h.take("calendar_search");
        require(search.request["params"].toObject()["calendars"].toArray() == QJsonArray{QJsonObject{{"account", "personal"}, {"calendar_id", "same-calendar"}}}, "account-only matches respect the existing selected catalog");
        h.complete(search, Harness::response(search.request, {event()}));
        auto row = h.backend.calendarSearch().value("events").toList().first().toMap();
        h.holdWindow = true; QVariantMap opened; int opens = 0;
        QObject::connect(&h.backend, &MailBackend::calendarSearchResultReady, [&](const auto &event) { opened = event; ++opens; });
        auto forged = row; forged["account"] = "workspace"; h.backend.openCalendarSearchResult(forged);
        require(!h.backend.calendarSearch().value("busy").toBool(), "only a retained result may navigate");
        h.backend.openCalendarSearchResult(row); auto read = h.take("calendar_combined");
        require(read.request["params"].toObject()["calendars"].isNull() && opened.isEmpty(), "fresh day query restores full saved calendar visibility before any details");
        h.complete(read, Harness::response(read.request, {event("personal", "Fresh title"), event("workspace")}));
        require(opens == 1 && opened.value("title") == "Fresh title" && opened.value("account") == "personal"
            && h.backend.m_account == "personal" && h.backend.m_events.size() == 2 && h.backend.m_calendarView == "day",
            "fresh exact occurrence opens and global cache retains the other account for Combined");
        require(h.backend.m_preferences.value("selected_calendars").isNull(), "view-local navigation never writes calendar visibility");
        h.backend.setCalendarSearchContext(false, false); h.backend.setCalendarSearchContext(true, false);
        search = h.take("calendar_search"); h.complete(search, Harness::response(search.request, {event()}));
        row = h.backend.calendarSearch().value("events").toList().first().toMap();
        h.backend.openCalendarSearchResult(row); read = h.take("calendar_combined");
        h.backend.setCalendarSearchContext(false, false);
        h.complete(read, Harness::response(read.request, {event("personal", "Late navigation")}));
        require(opens == 1 && h.backend.m_events[0].toMap().value("title") != "Late navigation", "closed popup fences a held selection and window publication");
        h.backend.setCalendarSearchContext(true, false); search = h.take("calendar_search");
        h.complete(search, Harness::response(search.request, {event()})); row = h.backend.calendarSearch().value("events").toList().first().toMap();
        h.backend.openCalendarSearchResult(row); read = h.take("calendar_combined");
        h.complete(read, Harness::response(read.request, {event("workspace")}));
        require(opens == 1 && h.backend.calendarSearch().value("status").toString().contains("no longer visible"), "same IDs in another account do not replace a missing occurrence");
        require(h.engine.operations == 0, "all search/selection work uses cached IPC, no Google operation");
    }
    static void controls() {
        Harness h; QTemporaryDir copy;
        // Main also instantiates Settings. Its unrelated cached-label read is
        // already in progress, as during normal startup; search never owns it.
        h.backend.m_googleLabelsAccount = "personal"; h.backend.m_googleLabelsLoading = true;
        const auto modulePath = copy.filePath("SearchTest"); require(QDir().mkpath(modulePath), "temporary QML module");
        QByteArray module("module SearchTest\nsingleton Theme 1.0 Theme.qml\n");
        for (const auto &name : QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"}, QDir::Files)) {
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + name, modulePath + '/' + name), "actual QML copied unchanged");
            if (name != "Theme.qml") module += name.chopped(4).toUtf8() + " 1.0 " + name.toUtf8() + '\n';
        }
        QFile registration(modulePath + "/qmldir"); require(registration.open(QIODevice::WriteOnly), "QML module registration"); registration.write(module); registration.close();
        qmlRegisterType<ReplyDocument>("SearchTest", 1, 0, "ReplyDocument"); qmlRegisterType<HatchItem>("SearchTest", 1, 0, "HatchItem");
        for (const auto &weight : {"Regular", "Medium", "SemiBold"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf") >= 0, "real bundled font");
        QGuiApplication::setFont(QFont("Geist", 10));
        QQmlApplicationEngine qml; bool warnings = false;
        QObject::connect(&qml, &QQmlEngine::warnings, [&](const auto &errors) { warnings = true; for (const auto &error : errors) qWarning() << error.toString(); });
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &h.backend);
        qml.loadData("import SearchTest 1.0\nMain { width: 1000; height: 700 }", QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!qml.rootObjects().isEmpty(), "actual Main and contextual search load");
        auto *window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); require(window, "native Main window");
        window->show();
        if (QGuiApplication::platformName() != "offscreen") { window->raise(); window->requestActivate(); }
        await([&] { return window->isExposed(); }, "native Main window is exposed");
        await([&] { return !named(window, "startupOverlay")->property("visible").toBool(); }, "actual startup overlay finishes before search input");
        auto *field = named(window, "searchField"); auto *calendar = named(window, "calendarWorkspace");
        require(field && calendar, "actual top search and Calendar workspace exist");
        h.backend.m_query = "mail-only words"; emit h.backend.queryChanged();
        require(QMetaObject::invokeMethod(window, "navigate", Q_ARG(QVariant, "calendar")), "actual Calendar navigation");
        require(field->property("text").toString().isEmpty(), "Calendar query starts independently from Mail");
        field->setProperty("text", "project"); require(QMetaObject::invokeMethod(field, "textEdited"), "actual top-field editing handler");
        auto search = h.take("calendar_search");
        require(h.backend.m_query == "mail-only words" && search.request["params"].toObject()["query"] == "project", "Calendar typing routes to shared search without changing Mail");
        auto longEvent = event("personal", "<b>Plain event title</b> — a long saved invitation with readable text and safe ellipsis 日本語");
        longEvent["calendar_name"] = "A very long saved Calendar name belonging to a separate Google account";
        auto result = Harness::response(search.request, {longEvent}); result["has_more"] = true;
        h.complete(search, result);
        auto *popup = named(window, "calendarSearchPopup"); require(popup && popup->property("opened").toBool(), "actual contextual popup opens");
        popup->setProperty("width", 390);
        const auto captures = qEnvironmentVariable("FM_CALENDAR_SEARCH_CAPTURE_DIR");
        if (!captures.isEmpty()) require(QDir().mkpath(captures), "capture directory");
        for (const auto theme : {"graphite", "paper", "harbour"}) for (const auto size : {"default", "large"}) {
            h.backend.m_preferences["theme"] = theme; h.backend.m_preferences["text_size"] = size; emit h.backend.preferencesChanged();
            bool painted = false; auto connection = QObject::connect(window, &QQuickWindow::frameSwapped, [&] { painted = true; });
            window->requestUpdate(); await([&] { return painted; }, "actual post-theme frame"); QObject::disconnect(connection);
            const auto frame = window->grabWindow();
            require(!frame.isNull(), "actual theme/font frame");
            auto *button = qobject_cast<QQuickItem *>(named(window, "calendarSearchResult_0"));
            auto *content = popup->property("contentItem").value<QQuickItem *>();
            require(button && content && button->isVisible() && button->width() > 200, "saved event row remains visible");
            const auto bounds = button->mapRectToItem(content, button->boundingRect());
            require(bounds.left() >= -.5 && bounds.right() <= content->width() + .5 && bounds.bottom() <= content->height() + .5,
                "narrow default/large result button stays inside popup");
            require(named(window, "calendarSearchRange")->property("text").toString().contains("2026-09-14 – 2026-09-20"), "civil inclusive date caption matches half-open week");
            require(!warnings, "all actual Main/search QML bindings are valid");
            if (!captures.isEmpty()) {
                const auto path = QDir(captures).filePath(QString("calendar-search-%1-%2.png").arg(theme, size));
                require(!QFile::exists(path) && frame.save(path), "retain unique themed popup image");
            }
        }
        QVariant lastDate; require(QMetaObject::invokeMethod(popup, "lastDate", Q_RETURN_ARG(QVariant, lastDate), Q_ARG(QVariant, "2026-03-09")), "civil date caption function");
        require(lastDate == "2026-03-08", "DST weekend caption uses civil subtraction");
        h.holdWindow = true; auto *button = named(window, "calendarSearchResult_0");
        require(QMetaObject::invokeMethod(button, "clicked"), "actual saved result click");
        auto fresh = h.take("calendar_combined"); h.complete(fresh, Harness::response(fresh.request, {event("personal", "Current permitted title"), event("workspace")}));
        await([&] { return !popup->property("opened").toBool(); }, "fresh selection closes only the search popup");
        auto *details = named(window, "eventDetails");
        require(details, "actual details popup exists");
        await([&] { return details->property("opened").toBool(); }, "actual details enter transition completes");
        require(details->property("eventData").toMap().value("title") == "Current permitted title", "details use the fresh shared row");
        const auto detail = details->property("eventData").toMap();
        require(!detail.value("can_edit").toBool() && !detail.value("can_delete").toBool(), "read-only source does not acquire mutation authority through search");
        require(QMetaObject::invokeMethod(details, "close"), "close exact details");
        require(QMetaObject::invokeMethod(window, "navigate", Q_ARG(QVariant, "mail")), "return to Mail");
        require(field->property("text") == "mail-only words" && h.backend.calendarSearchQuery() == "project", "context switching preserves both independent queries");
        require(!warnings && h.engine.operations == 0, "native search uses no provider operation and has no QML warnings");
    }
public:
    static void run() {
        contexts(); selection(); controls();
        qInfo() << "Calendar search passed: shared cached IPC, independent Main search, complete context fences, fresh exact occurrence navigation, full Combined visibility and three-theme/default-large controls. No provider or performance claim.";
    }
};
int main(int argc, char **argv) { QGuiApplication app(argc, argv); QQuickStyle::setStyle("Basic"); CalendarSearchTest::run(); return 0; }
