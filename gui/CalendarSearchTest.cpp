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
#include <QAccessible>
#include <QKeyEvent>
#include <QMouseEvent>

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
QJsonObject dayEvent(const QString &date, const QString &title) {
    auto row=event("personal",title);
    row["id"]=date; row["event_id"]=date; row["occurrence_key"]=date+"T09:00:00Z";
    row["start"]=date+"T09:00:00Z"; row["end"]=date+"T10:00:00Z";
    return row;
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
        QJsonArray catalog;
        bool holdWindow = false;
        Harness() {
            backend.m_account = "personal"; backend.m_enginePid = QCoreApplication::applicationPid();
            backend.m_calendarDate = "2026-09-14"; backend.m_timelineDate = "2026-09-14";
            backend.m_preferences = {{"notifications", "off"}, {"theme", "graphite"}, {"text_size", "default"},
                {"calendar_display_time_zone", "UTC"}, {"timeline_start_hour",7}, {"timeline_end_hour",21},
                {"selected_calendars", QVariant()}, {"last_workspace", "mail"}, {"startup_view", "mail"}};
            backend.m_preferencesSaving = false;
            for (const auto &id : {"personal", "workspace"}) {
                backend.m_allAccounts.append(QVariantMap{{"id", id}, {"name", id}, {"email", QString(id) + "@example.test"},
                    {"message_count",0},{"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}},
                    {"google", QVariantMap{{"connection_state", "connected"}, {"capabilities", QVariantMap{{"calendar_read", true}, {"mail_read", true}}}}}});
                backend.m_calendarSources.append(QVariantMap{{"account", id}, {"calendar_id", "same-calendar"},
                    {"name", "Saved team calendar"}, {"revision", 1}, {"permission", "reader"}, {"writable", false}});
            }
            require(backend.publishAccounts(backend.m_allAccounts), "stable account model");
            catalog = QJsonArray::fromVariantList(backend.m_calendarSources);
            engine.customRequest = [this](auto *peer, const auto &request) {
                const auto method = request["method"].toString(); methods.append(method);
                if (method == "calendar_search" || (method == "calendar_combined" && holdWindow)) requests.append({peer, request});
                else {
                    QJsonObject result;
                    if (method == "assist_settings_get") result = {{"revision",1},{"settings",QJsonObject{}},{"defaults",QJsonObject{}}};
                    else if (method == "assist_results") result = {{"items",QJsonArray{}}};
                    else if (method == "assist_watch") result = {{"watching",false}};
                    else if (method == "assist_jobs") result = {{"jobs",QJsonArray{}}};
                    else if (method == "google_sync_refresh") {
                        require(request["account"] == "personal" && request["params"].toObject().isEmpty(), "header uses exact account and built-in limits");
                        result = {{"active", true}};
                    }
                    else if (method == "google_sync_status") result = {{"accounts", QJsonArray::fromVariantList(hSyncRows())}};
                    else if (method == "preferences_save") result = {{"revision",backend.m_preferencesRevision+1},{"preferences",QJsonObject::fromVariantMap(backend.m_preferences)}};
                    else if (method == "calendar_combined") result = response(request, current);
                    else if (method == "calendar_mail_activity") result = {{"state", "ready"}, {"days", QJsonArray{}}};
                    else if (method == "calendar_sources") result = {{"calendars", catalog}};
                    else if (method == "accounts") result = {{"accounts", QJsonArray::fromVariantList(backend.m_allAccounts)}};
                    else if (method == "view_get") result = {{"revision", 0}, {"view", QJsonObject{}}};
                    else if (method == "view_save") result = {{"revision", 1}};
                    else if (method == "conversations") result = {{"conversations", QJsonArray{}}, {"revision", 1}};
                    else if (method == "categories") result = {{"categories", QJsonArray{}}};
                    else if (method == "label_catalog") result = {{"account", request["account"]}, {"labels", QJsonArray{}}};
                    else if (method == "google_sent") result = {{"account", request["account"]}, {"outgoing", QJsonArray{}}, {"next_offset", QJsonValue::Null}};
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
        QVariantList hSyncRows() const { QVariantList rows; for (const auto &v : backend.m_googleSync) rows.append(v); return rows; }
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
    static void timelineNavigation() {
        Harness h; h.holdWindow=true;
        h.current={dayEvent("2026-09-13","Previous day"),dayEvent("2026-09-14","Selected day"),dayEvent("2026-09-15","Next day")};
        auto overnight=dayEvent("2026-09-14","Overnight"); overnight["id"]="overnight"; overnight["start"]="2026-09-14T23:00:00Z"; overnight["end"]="2026-09-15T08:00:00Z"; h.current.append(overnight);
        h.backend.loadCalendar();
        auto calendar=h.take("calendar_combined"); h.complete(calendar,Harness::response(calendar.request,h.current));
        auto strip=h.take("calendar_combined");
        const auto params=strip.request["params"].toObject();
        require(params["from"]=="2026-09-13" && params["to"]=="2026-09-16","one bounded three-day timeline query");
        h.complete(strip,Harness::response(strip.request,h.current));
        auto panels=h.backend.timelinePanels();
        require(panels.size()==3 && panels[0].toMap()["events"].toList()[0].toMap()["title"]=="Previous day"
            && panels[2].toMap()["events"].toList().size()==2,"neighbors contain their own clipped day events");
        h.backend.moveTimeline(1);
        require(h.backend.timelineReady() && h.backend.todayEvents().size()==2 && h.backend.timelineDate()=="2026-09-15","adjacent navigation keeps its cached events while refreshing");
        require(h.backend.m_timelineDayEvents.size()==2 && !h.backend.timelinePanels()[2].toMap()["ready"].toBool(),"evict the old outer day and mark the new outer day loading");
        calendar=h.take("calendar_combined"); h.complete(calendar,Harness::response(calendar.request,h.current));
        strip=h.take("calendar_combined");
        h.backend.m_preferences["calendar_display_time_zone"]="America/Toronto";
        h.backend.loadCalendar();
        require(h.backend.todayEvents().isEmpty() && h.backend.m_timelineDayEvents.isEmpty(),"display zone changes invalidate all cached day geometry");
        h.complete(strip,Harness::response(strip.request,h.current));
        require(h.backend.m_timelineDayEvents.isEmpty(),"late timeline result cannot restore prior-zone geometry");
        calendar=h.take("calendar_combined"); h.complete(calendar,Harness::response(calendar.request,{}));
        strip=h.take("calendar_combined"); h.complete(strip,Harness::response(strip.request,{}));
        require(h.backend.m_timelineDayEvents.size()==3,"timeline retains at most the three requested days");
    }
    static void accountVisibility() {
        Harness h; h.holdWindow = true;
        h.backend.m_preferences["selected_calendars"] = QVariantList{
            QVariantMap{{"account","personal"},{"calendar_id","same-calendar"}},
            QVariantMap{{"account","workspace"},{"calendar_id","same-calendar"}}};
        const auto storedSelection=h.backend.m_preferences["selected_calendars"];
        h.backend.publishCalendarWindow({{"events",h.current},{"time_zone","UTC"}},"2026-09-14",7);
        h.backend.m_todayEvents=h.backend.m_events;
        h.backend.m_reminders=h.backend.m_events;
        h.backend.setPreferences({{"calendar_account_visibility","current"}});
        require(h.backend.events().size()==1 && h.backend.todayEvents().size()==1,"Hide applies immediately to cached Calendar and timeline");
        require(h.backend.reminders().size()==2,"view visibility does not suppress configured reminders");
        const auto oldCalendar=h.take("calendar_combined");
        const QJsonArray personal{QJsonObject{{"account","personal"},{"calendar_id","same-calendar"}}};
        require(oldCalendar.request["params"].toObject()["calendars"].toArray()==personal,"Calendar uses current-account selection");
        h.backend.m_account="workspace"; emit h.backend.accountChanged();
        const QJsonArray workspace{QJsonObject{{"account","workspace"},{"calendar_id","same-calendar"}}};
        // The production socket sends one request at a time. Switch accounts
        // while the old query is held, then drain its queued timeline reply.
        h.complete(oldCalendar,Harness::response(oldCalendar.request,{event()}));
        const auto oldTimeline=h.take("calendar_combined");
        require(oldTimeline.request["params"].toObject()["calendars"].toArray()==personal,"timeline shares current-account selection");
        h.complete(oldTimeline,Harness::response(oldTimeline.request,{event()}));
        require(h.backend.events().isEmpty() && h.backend.todayEvents().isEmpty(),"late prior-account replies cannot restore hidden events");
        const auto nextCalendar=h.take("calendar_combined");
        require(nextCalendar.request["params"].toObject()["calendars"].toArray()==workspace,"Calendar visibility follows account changes");
        h.complete(nextCalendar,Harness::response(nextCalendar.request,{event("workspace")}));
        const auto nextTimeline=h.take("calendar_combined");
        require(nextTimeline.request["params"].toObject()["calendars"].toArray()==workspace,"timeline visibility follows account changes");
        h.complete(nextTimeline,Harness::response(nextTimeline.request,{event("workspace")}));
        require(h.backend.events().size()==1 && h.backend.events()[0].toMap()["account"]=="workspace","current account reply populates Calendar");
        require(h.backend.calendarSearchParams()["calendars"].toArray()==workspace,"contextual search respects visible accounts");
        require(h.backend.m_preferences["selected_calendars"]==storedSelection,"Show/Hide preserves individual calendar choices");
        h.backend.setPreferences({{"calendar_account_visibility","all"}});
        const auto allCalendar=h.take("calendar_combined");
        require(allCalendar.request["params"].toObject()["calendars"].toArray()==QJsonArray::fromVariantList(storedSelection.toList()),"Show restores the full saved selection");
        h.complete(allCalendar,Harness::response(allCalendar.request,h.current));
        const auto allTimeline=h.take("calendar_combined");
        h.complete(allTimeline,Harness::response(allTimeline.request,h.current));
        h.backend.m_preferences["calendar_account_visibility"]="current";
        h.backend.m_preferences["selected_calendars"]=QVariantList{};
        require(h.backend.effectiveCalendarSelection()==QJsonArray{},"an empty calendar selection stays empty");
        h.backend.m_preferences["selected_calendars"]=QVariant{};
        require(h.backend.effectiveCalendarSelection()==workspace,"all-source selection resolves current account from the catalog");
    }
    static void catalogPolling() {
        Harness h;
        h.backend.m_syntheticProfile = true; // Isolate local generation polling from unrelated provider status.
        h.engine.statusResult = {{"revision", 1}, {"calendar_generation", 5}};
        h.backend.m_revision = 1; h.backend.m_calendarRevision = 4;
        h.backend.m_calendarSources.clear();
        require(QMetaObject::invokeMethod(&h.backend.m_pollTimer, "timeout"), "run the actual status polling handler");
        await([&] { return h.backend.m_calendarSources.size() == 2 && h.backend.m_pending.isEmpty(); }, "first sync publishes the catalog to the already-connected UI");
        const auto reads = h.methods.count("calendar_sources");
        require(reads == 1, "a changed Calendar generation queries its sources");
        require(QMetaObject::invokeMethod(&h.backend.m_pollTimer, "timeout"), "poll unchanged generation");
        await([&] { return h.backend.m_pending.isEmpty(); }, "unchanged status completes");
        require(h.methods.count("calendar_sources") == reads, "unchanged generations do not repeatedly query the catalog");
        int notifications = 0;
        QObject::connect(&h.backend, &MailBackend::eventsChanged, [&] { ++notifications; });
        h.backend.loadCalendarSources();
        await([&] { return h.backend.m_pending.isEmpty(); }, "same catalog read completes");
        require(notifications == 0, "identical catalog does not disturb open controls or cached search");
        const auto epoch = h.backend.m_calendarAuthorityEpoch;
        auto observed = h.catalog[0].toObject(); observed["revision"] = 2; h.catalog[0] = observed;
        h.backend.loadCalendarSources(); await([&] { return h.backend.m_pending.isEmpty(); }, "permission observation reaches catalog");
        require(h.backend.m_calendarAuthorityEpoch == epoch, "unchanged permission observations do not invalidate their own occurrence lookup");
        observed["permission"] = "none"; h.catalog[0] = observed;
        h.backend.loadCalendarSources(); await([&] { return h.backend.m_pending.isEmpty(); }, "permission revocation reaches catalog");
        observed["permission"] = "reader"; h.catalog[0] = observed;
        h.backend.loadCalendarSources(); await([&] { return h.backend.m_pending.isEmpty(); }, "permission restoration reaches catalog");
        require(h.backend.m_calendarAuthorityEpoch == epoch + 2, "revocation and restoration retain distinct UI authority epochs");
        h.catalog.removeAt(0); h.engine.statusResult["calendar_generation"] = 6;
        require(QMetaObject::invokeMethod(&h.backend.m_pollTimer, "timeout"), "poll withdrawn calendar");
        await([&] { return h.backend.m_calendarSources.size() == 1 && h.backend.m_pending.isEmpty(); }, "catalog withdrawal reaches current UI");
    }
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
            if(name=="ShortcutRegistry.qml") module+="singleton ";
            if(name!="Theme.qml") module+= name.chopped(4).toUtf8() + " 1.0 " + name.toUtf8() + '\n';
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
        auto *sync = named(window, "headerSyncButton");
        require(sync && sync->property("text").toString() == "Sync", "header has a real Sync control");
        h.backend.m_syntheticProfile = false; emit h.backend.connectedChanged();
        await([&]{return sync->property("enabled").toBool();}, "live account can sync");
        h.backend.m_googleSync = {{"personal", QVariantMap{{"account","personal"},{"active",true},{"last_mail_sync_at",1789408800LL},
            {"credential_access_required",true},{"progress",QVariantMap{{"mail","paused"}}}}}};
        emit h.backend.googleSyncChanged();
        require(sync->property("text").toString().startsWith("Retry sync · "), "denial has explicit retry and keeps last success");
        auto *syncAccessible=QAccessible::queryAccessibleInterface(sync);
        require(syncAccessible && syncAccessible->actionInterface(), "sync accessible activation");
        syncAccessible->actionInterface()->doAction(QAccessibleActionInterface::pressAction());
        await([&]{return h.methods.contains("google_sync_refresh") && !h.backend.m_googleSyncChanging.contains("personal");}, "real button sends refresh RPC");
        auto calendarOnly = h.backend.m_allAccounts;
        auto personal = calendarOnly[0].toMap();
        auto google = personal["google"].toMap();
        google["capabilities"] = QVariantMap{{"calendar_read", true}, {"mail_read", false}};
        personal["google"] = google; calendarOnly[0] = personal;
        require(h.backend.publishAccounts(calendarOnly), "calendar-only account model updates"); emit h.backend.accountsChanged();
        require(h.backend.accountCapability("personal", "calendar_read") && !h.backend.accountCapability("personal", "mail_read"), "calendar-only capability reaches native backend");
        await([&]{return sync->property("enabled").toBool() && !sync->property("text").toString().contains(" · ");}, qPrintable("calendar-only Sync stays available without a mail timestamp: " + sync->property("text").toString() + " enabled=" + QString::number(sync->property("enabled").toBool())));
        const auto refreshes = h.methods.count("google_sync_refresh");
        syncAccessible->actionInterface()->doAction(QAccessibleActionInterface::pressAction());
        await([&]{return h.methods.count("google_sync_refresh") == refreshes + 1 && !h.backend.m_googleSyncChanging.contains("personal");}, "calendar-only Sync sends refresh RPC");
        require(h.backend.publishAccounts(h.backend.m_allAccounts), "restore mail and Calendar account model"); emit h.backend.accountsChanged();
        auto row=h.backend.m_googleSync["personal"].toMap(); row["credential_access_required"]=false;
        row["progress"]=QVariantMap{{"mail","running"}}; h.backend.m_googleSync["personal"]=row; emit h.backend.googleSyncChanged();
        require(sync->property("text").toString() == "Syncing…" && sync->property("tip").toString().contains("Last successful mail sync: "), "running label matches the design; the tip keeps the previous successful time");
        h.backend.m_account="workspace"; emit h.backend.accountChanged();
        require(sync->property("lastSync").toDouble()==0, "sync timestamp cannot cross accounts");
        h.backend.m_account="personal"; emit h.backend.accountChanged();
        require(QMetaObject::invokeMethod(window,"navigate",Q_ARG(QVariant,"settings")),"open production Settings");
        auto *settings=named(window,"settingsView"); require(settings,"production Settings exists");
        settings->setProperty("page","Calendar");
        QCoreApplication::processEvents();
        auto pressSetting=[&](const char *name) {
            auto *button=named(window,name);
            require(button && button->property("visible").toBool(),"Calendar setting is visible");
            auto *accessible=QAccessible::queryAccessibleInterface(button);
            require(accessible && accessible->actionInterface(),"Calendar setting has accessible activation");
            accessible->actionInterface()->doAction(QAccessibleActionInterface::pressAction());
            await([&]{return !h.backend.m_preferencesSaving;},"Calendar preference save completes");
        };
        require(!named(window,"calendarDisplayTimeZone")->property("visible").toBool(),"Calendar details start collapsed");
        // Account visibility is the Calendar header's "All calendars" switch, as in the design.
        require(!named(window,"calendarAccountVisibility"),"Settings does not duplicate the Calendar account switch");
        const auto settingsCaptures=qEnvironmentVariable("FM_CALENDAR_SETTINGS_CAPTURE_DIR");
        if (!settingsCaptures.isEmpty()) {
            require(QDir().mkpath(settingsCaptures),"Calendar Settings capture directory");
            for (const auto theme:{"graphite","paper","terminal"}) {
                h.backend.m_preferences["theme"]=theme; emit h.backend.preferencesChanged();
                bool painted=false; auto frame=QObject::connect(window,&QQuickWindow::frameSwapped,[&]{painted=true;});
                window->requestUpdate(); await([&]{return painted;},"Calendar Settings frame settles"); QObject::disconnect(frame);
                require(window->grabWindow().save(QDir(settingsCaptures).filePath(QString("calendar-settings-%1.png").arg(theme))),"Calendar Settings theme capture");
            }
        }
        pressSetting("calendarDisplayButton");
        require(named(window,"calendarDisplayTimeZone")->property("visible").toBool(),"display zone remains available in expanded details");
        pressSetting("calendarDisplayButton");
        require(QMetaObject::invokeMethod(window,"navigate",Q_ARG(QVariant,"mail")),"return from Settings");
        h.backend.m_syntheticProfile=true; emit h.backend.connectedChanged();
        require(!sync->property("enabled").toBool(), "synthetic profile cannot start sync");
        auto *field = named(window, "searchField"); auto *calendar = named(window, "calendarWorkspace");
        require(field && calendar, "actual top search and Calendar workspace exist");
        auto *searchField = qobject_cast<QQuickItem *>(field);
        auto *mailPopup = named(window, "mailSearchPopup");
        require(searchField && mailPopup, "native Mail search controls exist");
        searchField->forceActiveFocus(Qt::TabFocusReason);
        await([&] { return mailPopup->property("opened").toBool(); }, "deliberate keyboard focus opens Mail search");
        auto pressSearch = [&](const char *name) {
            auto *button = named(window, name);
            require(button && button->property("visible").toBool() && button->property("enabled").toBool(), "search action is visible and enabled");
            require(QMetaObject::invokeMethod(button, "clicked"), "production search action handler");
            QCoreApplication::processEvents();
        };
        pressSearch("searchQuickAttachment");
        require(h.backend.filters().value("has_attachment").toBool(), "quick attachment chip applies a real filter");
        pressSearch("searchQuickAttachment");
        require(!h.backend.filters().contains("has_attachment"), "quick filter toggles off");
        pressSearch("searchQuickDelayed");
        require(h.backend.filters().value("status") == "held", "Delayed chip uses the existing review filter");
        pressSearch("searchQuickBefore");
        require(h.backend.filters().value("before").toString().endsWith("-01-01"), "before chip applies a real date bound");
        require(h.backend.query().isEmpty(), "quick filters preserve the user's search words");
        h.backend.applyFilters({});
        pressSearch("searchScope_sent");
        require(!named(window, "searchQuickAttachment")->property("visible").toBool(), "Sent hides Inbox quick filters");
        pressSearch("searchSentToday");
        const auto todayBound = h.backend.filters().value("after").toString();
        const auto tomorrowBound = h.backend.filters().value("before").toString();
        require(QDate::fromString(todayBound, Qt::ISODate).daysTo(QDate::fromString(tomorrowBound, Qt::ISODate)) == 1, "Today is one complete UTC day");
        pressSearch("searchSentYesterday");
        require(h.backend.filters().value("before").toString() == todayBound, "Yesterday ends at today's boundary");
        pressSearch("searchSentEarlier");
        require(!h.backend.filters().contains("after") && h.backend.filters().value("before").toString() < todayBound, "Earlier has only an exclusive upper bound");
        pressSearch("searchSentEarlier");
        require(!h.backend.filters().contains("before"), "selected Sent date toggles off");
        pressSearch("searchScope_inbox");
        bool widePainted=false; auto wideFrame=QObject::connect(window,&QQuickWindow::frameSwapped,[&]{widePainted=true;});
        window->resize(1400,700); window->requestUpdate();
        await([&]{return widePainted;},"wide search layout settles"); QObject::disconnect(wideFrame);
        require(QMetaObject::invokeMethod(window,"openSearch"),"search anchors to the wide window");
        const auto widePopupX=mailPopup->property("x").toDouble();
        require(widePopupX+mailPopup->property("width").toDouble()>1000,"wide search popup needs repositioning at the smaller width");
        window->resize(1000,700);
        await([&]{return mailPopup->property("x").toDouble()+mailPopup->property("width").toDouble()<=988;},"open search follows the smaller viewport");
        require(mailPopup->property("opened").toBool() && searchField->hasActiveFocus(),"resize preserves the open popup and typing focus");
        const auto mailCaptures = qEnvironmentVariable("FM_MAIL_SEARCH_CAPTURE_DIR");
        if (!mailCaptures.isEmpty()) {
            require(QDir().mkpath(mailCaptures), "mail search capture directory");
            h.backend.m_categories = {QVariantMap{{"id","planning"},{"name","Planning"},{"conversation_count",0}},QVariantMap{{"id","finance"},{"name","Finance"},{"conversation_count",0}},QVariantMap{{"id","infra"},{"name","Infrastructure"},{"conversation_count",0}}};
            emit h.backend.categoriesChanged();
            mailPopup->setProperty("senderOptions", QVariantList{QVariantMap{{"name","Maya Chen"},{"email","maya@example.test"}},QVariantMap{{"name","North American infrastructure planning and operations team"},{"email","planning@example.test"}}});
            for (const auto theme : {"graphite", "paper", "terminal"}) {
                h.backend.m_preferences["theme"] = theme; emit h.backend.preferencesChanged();
                bool painted = false; auto connection = QObject::connect(window, &QQuickWindow::frameSwapped, [&] { painted = true; });
                window->requestUpdate(); await([&] { return painted; }, "mail search post-theme frame"); QObject::disconnect(connection);
                require(window->grabWindow().save(QDir(mailCaptures).filePath(QString("mail-search-%1.png").arg(theme))), "mail search theme capture");
            }
            h.backend.m_preferences["theme"]="paper"; h.backend.m_preferences["text_size"]="large"; emit h.backend.preferencesChanged();
            mailPopup->setProperty("width",350);
            bool painted=false; auto connection=QObject::connect(window,&QQuickWindow::frameSwapped,[&]{painted=true;});
            window->requestUpdate(); await([&]{return painted;},"narrow mail search frame"); QObject::disconnect(connection);
            require(window->grabWindow().save(QDir(mailCaptures).filePath("mail-search-narrow-paper.png")),"narrow mail search capture");
            mailPopup->setProperty("width",440);
            h.backend.m_preferences["text_size"]="default"; emit h.backend.preferencesChanged();
        }
        QKeyEvent escapePress(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
        QCoreApplication::sendEvent(window, &escapePress);
        QKeyEvent escapeRelease(QEvent::KeyRelease, Qt::Key_Escape, Qt::NoModifier);
        QCoreApplication::sendEvent(window, &escapeRelease);
        await([&] { return !mailPopup->property("visible").toBool(); }, "Escape dismisses search while its text field owns focus");
        window->contentItem()->forceActiveFocus(Qt::PopupFocusReason);
        searchField->forceActiveFocus(Qt::PopupFocusReason);
        QCoreApplication::processEvents();
        require(!mailPopup->property("visible").toBool(), "restored popup focus does not reopen dismissed search");
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
        for (const auto theme : {"graphite", "paper", "terminal"}) for (const auto size : {"default", "large"}) {
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
        await([&] { return !details->property("visible").toBool(); }, "details close before accessible activation");
        const auto press = [&](const QString &name) {
            auto *item = named(window, name); require(item, "actual accessible calendar block exists");
            auto *accessible = QAccessible::queryAccessibleInterface(item);
            require(accessible && accessible->actionInterface(), "calendar block exposes accessible actions");
            accessible->actionInterface()->doAction(QAccessibleActionInterface::pressAction());
        };
        press("calendarEvent-instance");
        await([&] { return details->property("opened").toBool(); }, "accessible press opens the timed event");
        require(QMetaObject::invokeMethod(details, "close"), "close timed event");
        await([&] { return !details->property("visible").toBool(); }, "timed details close");
        h.backend.m_todayEvents = h.backend.m_events; h.backend.m_timelineReady = true; emit h.backend.eventsChanged();
        require(QMetaObject::invokeMethod(window, "navigate", Q_ARG(QVariant, "mail")), "return to Mail");
        require(field->property("text") == "mail-only words" && h.backend.calendarSearchQuery() == "project", "context switching preserves both independent queries");
        press("timelineEvent-instance");
        QObject *timelineDetails = nullptr;
        for (auto *candidate : window->findChildren<QObject *>("eventDetails"))
            if (candidate->property("timelineSource").toBool()) timelineDetails = candidate;
        require(timelineDetails, "Mail timeline owns its details popup");
        await([&] { return timelineDetails->property("opened").toBool(); }, "accessible press opens the Mail timeline event");
        require(QMetaObject::invokeMethod(timelineDetails, "close"), "close timeline details");
        h.holdWindow = false;
        h.current={dayEvent("2026-09-13","Previous day"),dayEvent("2026-09-14","Selected day"),dayEvent("2026-09-15","Next day")};
        h.backend.loadCalendar();
        await([&]{return h.backend.m_pending.isEmpty() && h.backend.timelineReady();},"populate adjacent days through production backend");
        auto *strip=qobject_cast<QQuickItem *>(named(window,"mailTimeline")); require(strip,"native Mail timeline");
        auto *nextPanel=named(window,"timelinePanelNext");
        require(nextPanel && !nextPanel->property("enabled").toBool(),"offscreen neighbor cannot receive input");
        auto mouse=[&](QEvent::Type type,qreal x,Qt::MouseButton button,Qt::MouseButtons buttons) {
            const auto local=strip->mapToScene(QPointF(x,15));
            QMouseEvent ev(type,local,window->mapToGlobal(local.toPoint()),button,buttons,Qt::NoModifier);
            QCoreApplication::sendEvent(window,&ev);
        };
        mouse(QEvent::MouseButtonPress,strip->width()*.7,Qt::LeftButton,Qt::LeftButton);
        mouse(QEvent::MouseMove,strip->width()*.1,Qt::NoButton,Qt::LeftButton);
        require(strip->property("dragOffset").toDouble() < -strip->width()*.5 && nextPanel->property("x").toDouble()<strip->width()*.5,"native drag reveals the real adjacent panel");
        const auto timelineCaptures=qEnvironmentVariable("FM_TIMELINE_CAPTURE_DIR");
        if (!timelineCaptures.isEmpty()) {
            require(QDir().mkpath(timelineCaptures),"timeline capture directory");
            for (const auto theme:{"graphite","paper","terminal"}) {
                h.backend.m_preferences["theme"]=theme; emit h.backend.preferencesChanged();
                bool painted=false; auto frame=QObject::connect(window,&QQuickWindow::frameSwapped,[&]{painted=true;});
                window->requestUpdate(); await([&]{return painted;},"dragged timeline frame"); QObject::disconnect(frame);
                require(window->grabWindow().save(QDir(timelineCaptures).filePath(QString("timeline-drag-%1.png").arg(theme))),"adjacent timeline capture");
            }
        }
        mouse(QEvent::MouseButtonRelease,strip->width()*.1,Qt::LeftButton,Qt::NoButton);
        await([&]{return h.backend.timelineDate()=="2026-09-15" && strip->property("dragOffset").toDouble()==0;},"drag commits one adjacent day and recenters");
        require(h.backend.todayEvents().size()==1 && h.backend.todayEvents()[0].toMap()["title"]=="Next day","navigation retains the painted neighbor's events");
        require(QMetaObject::invokeMethod(window, "navigate", Q_ARG(QVariant, "calendar")), "return to Calendar for all-day activation");
        auto allDay = event(); allDay["event_id"] = "all-day"; allDay["all_day"] = true;
        allDay["start"] = "2026-09-14"; allDay["end"] = "2026-09-15";
        h.backend.publishCalendarWindow({{"events", QJsonArray{allDay}}, {"time_zone", "UTC"}}, "2026-09-14", 1);
        emit h.backend.eventsChanged();
        press("calendarAllDay-all-day");
        await([&] { return details->property("opened").toBool(); }, "accessible press opens the all-day event");
        require(QMetaObject::invokeMethod(details, "close"), "close all-day details");
        require(!warnings && h.engine.operations == 0, "native search uses no provider operation and has no QML warnings");
    }
public:
    static void run() {
        accountVisibility(); timelineNavigation(); catalogPolling(); contexts(); selection(); controls();
        qInfo() << "Calendar search passed: shared cached IPC, independent Main search, complete context fences, fresh exact occurrence navigation, full Combined visibility and three-theme/default-large controls. No provider or performance claim.";
    }
};
int main(int argc, char **argv) { QGuiApplication app(argc, argv); QQuickStyle::setStyle("Basic"); CalendarSearchTest::run(); return 0; }
