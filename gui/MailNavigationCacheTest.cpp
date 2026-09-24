#include "MailBackend.h"
#include "FmIconItem.h"
#include "ReplyDocument.h"
#include "GoogleOperationTestFixture.h"
#include <QFontDatabase>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QQuickItem>
#include <QDebug>
#include <QInputMethodEvent>
#include <QJsonArray>
#include <QMouseEvent>
#include <QKeyEvent>
#include <cstdlib>

namespace {
void require(bool ok, const char *message) { if (!ok) { qCritical() << message; std::exit(1); } }
QVariantMap account(const QString &id, int revision = 1, const QString &state = "active") {
    return {{"id",id},{"name",id},{"email",id+"@example.test"},{"message_count",0},{"signature_text","Original signature"},{"metadata_revision",1},{"lifecycle",QVariantMap{{"state",state},{"revision",revision}}}};
}
QVariantMap reader(const QString &body, int revision) {
    return {{"revision",revision},{"messages",QVariantList{QVariantMap{{"id","message"},{"body",body},{"unread",true}}}}};
}
}

class MailNavigationCacheTest {
    static void visibleRefresh() {
        using GoogleOperationFixture::await;
        GoogleOperationFixture::Engine syncEngine;
        MailBackend backend(syncEngine.profile, "unused");
        backend.m_shutdown = true;
        backend.m_allAccounts = {account("one")}; backend.publishAccounts(backend.m_allAccounts);
        backend.m_account = "one"; backend.m_selected = "row-13";
        backend.m_preferences = {{"notifications", "off"}, {"theme", "paper"}, {"text_size", "default"}};
        backend.m_views["one"] = QVariantMap{{"listY", 0}};
        const auto makeRow = [](int number) {
            const auto id = "row-" + QString::number(number);
            return QVariantMap{{"id", id}, {"account", "one"}, {"subject", "Unique visible subject " + id},
                {"sender_name", "Sender " + id}, {"sender_email", "sender@example.test"}, {"snippet", "Body " + id},
                {"latest_at", "2026-09-11T23:00:00Z"}, {"kind", "unknown"}, {"categories", QVariantList{}},
                {"group", "review"}, {"unread", true}, {"starred", false}, {"message_count", 1}};
        };
        QVariantList before;
        for (int number : {0, 3, 5, 7, 9, 11, 13, 15, 18, 20, 21}) before.append(makeRow(number));
        for (int number = 22; before.size() < 100; ++number) before.append(makeRow(number));
        require(backend.publishConversations(before), "initial observed 100-row page");
        QTemporaryDir copy; require(copy.isValid(), "temporary unchanged MailView module");
        const auto modulePath = copy.filePath("NavigationTest"); require(QDir().mkpath(modulePath), "MailView module directory");
        QByteArray module("module NavigationTest\nsingleton Theme 1.0 Theme.qml\n");
        for (const auto &name : QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"}, QDir::Files)) {
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + name, modulePath + '/' + name), "copy actual QML unchanged");
            if(name=="ShortcutRegistry.qml") module+="singleton ";
            if(name!="Theme.qml") module+= name.chopped(4).toUtf8() + " 1.0 " + name.toUtf8() + '\n';
        }
        QFile registration(modulePath + "/qmldir"); require(registration.open(QIODevice::WriteOnly), "QML registration");
        registration.write(module); registration.close();
        qmlRegisterType<ReplyDocument>("NavigationTest", 1, 0, "ReplyDocument");
        for (const auto &weight : {"Regular", "Medium", "SemiBold"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf") >= 0, "actual bundled font");
        QGuiApplication::setFont(QFont("Geist", 10));
        QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings = false;
        QObject::connect(&qml, &QQmlEngine::warnings, [&](const auto &errors) { warnings = true; for (const auto &error : errors) qWarning() << error.toString(); });
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport NavigationTest 1.0\nApplicationWindow { visible: true; width: 1440; height: 700; color: Theme.canvas; palette.text: Theme.text; palette.base: Theme.surface; palette.window: Theme.canvas; palette.button: Theme.surface; palette.buttonText: Theme.text; property bool singleKeysAllowed: true; property bool searchPopupOpen: false; Binding { target: Theme; property: 'themeId'; value: backend.preferences.theme } Binding { target: Theme; property: 'textSize'; value: backend.preferences.text_size } MailView { objectName: \"testMail\"; anchors.fill: parent } SettingsView { anchors.fill: parent; visible: false } }", QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!qml.rootObjects().isEmpty(), "actual MailView loads");
        auto *window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); require(window, "native MailView window");
        await([&] { return window->isExposed(); }, "MailView is exposed");
        const auto frame = [&] {
            bool painted = false; const auto connection = QObject::connect(window, &QQuickWindow::frameSwapped, window, [&] { painted = true; });
            window->requestUpdate(); await([&] { return painted; }, "post-publication native frame"); QObject::disconnect(connection);
        };
        const auto rows = [&] {
            QList<QQuickItem *> result;
            const auto visit = [&](auto &&self, QQuickItem *item) -> void {
                if (!item->isVisible()) return;
                if (item->objectName().startsWith("conversationRow_")) {
                    auto bounds = item->mapRectToScene(item->boundingRect());
                    for (auto *parent = item->parentItem(); parent; parent = parent->parentItem())
                        if (parent->clip()) bounds = bounds.intersected(parent->mapRectToScene(parent->boundingRect()));
                    if (bounds.width() > 16 && bounds.height() > 8) result.append(item);
                }
                for (auto *child : item->childItems()) self(self, child);
            };
            visit(visit, window->contentItem());
            std::sort(result.begin(), result.end(), [](auto *left, auto *right) { return left->mapToScene({}).y() < right->mapToScene({}).y(); });
            return result;
        };
        frame();
        QPointer<QQuickItem> pinned, removed;
        for (auto *item : rows()) {
            if (item->objectName() == "conversationRow_row-13") pinned = item;
            if (item->objectName() == "conversationRow_row-15") removed = item;
        }
        require(pinned, "selected row is actually visible before structural refresh");
        QVariantList after;
        for (int number = 22; number < 222; ++number) after.append(makeRow(number));
        auto selected = makeRow(13); selected["pinned"] = true; selected["display_group"] = "review"; after.insert(6, selected);
        QQuickItem *list = nullptr;
        const auto findList = [&](auto &&self, QQuickItem *item) -> void {
            if (item->objectName() == "conversationList") list = item;
            for (auto *child : item->childItems()) self(self, child);
        };
        findList(findList, window->contentItem()); require(list, "actual ListView");
        for (int pass = 0; pass < 9; ++pass) {
            const auto &expected = pass % 2 ? before : after;
            require(backend.publishConversations(expected), "repeat forward/backward pinned page refresh"); emit backend.conversationsChanged();
            frame();
            QSet<QString> visibleIds; int visibleIndex = 0; qreal bottom = -1;
            for (auto *item : rows()) {
                const auto id = item->objectName().mid(QString("conversationRow_").size());
                const auto index = item->parentItem()->property("index").toInt(); QQuickItem *owned = nullptr;
                require(QMetaObject::invokeMethod(list, "itemAtIndex", Q_RETURN_ARG(QQuickItem *, owned), Q_ARG(int, index)), "current public ListView identity");
                require(index >= 0 && index < expected.size() && expected[index].toMap().value("id") == id && !visibleIds.contains(id),
                    "every painted row has one exact current model identity and index");
                const auto rect = item->mapRectToScene(item->boundingRect());
                require(owned == item->parentItem() && index == visibleIndex++ && rect.top() >= bottom,
                    "owned visible rows follow the current page without gaps or overlapping retired rows");
                bottom = rect.bottom(); visibleIds.insert(id);
            }
            require(pinned && pinned->isVisible() && pinned->isEnabled() && visibleIds.contains("row-13"), "selected surviving delegate remains stable and visible");
            require(!removed || (!removed->isVisible() && !removed->isEnabled()), "retired original delegate cannot paint or accept clicks");
            qInfo() << "Actual MailView pinned refresh" << pass << expected.size() << "rows; exact visible order" << visibleIndex;
        }
        const auto captureDir = qEnvironmentVariable("FM_LIST_REFRESH_CAPTURE_DIR");
        if (!captureDir.isEmpty()) {
            require(QDir().mkpath(captureDir), "diagnostic capture directory");
            const auto path = captureDir + "/list-refresh.png";
            require(!QFile::exists(path) && window->grabWindow().save(path), "retain actual final list pixels");
        }
        auto *settings = window->findChild<QQuickItem *>("settingsView");
        require(settings, "actual Settings view exists");
        settings->setProperty("page", "Mail"); settings->setProperty("signaturesOpen", true); settings->setVisible(true);
        QQuickItem *signature = nullptr;
        const auto findSignature = [&](auto &&self, QQuickItem *item) -> void {
            if (item->objectName() == "signature_one") signature = item;
            for (auto *child : item->childItems()) self(self, child);
        };
        findSignature(findSignature, window->contentItem()); require(signature, "actual signature editor");
        QPointer<QQuickItem> editor(signature); signature->forceActiveFocus();
        QMetaObject::invokeMethod(signature, "selectAll");
        QInputMethodEvent first; first.setCommitString("Regards\nTest"); QCoreApplication::sendEvent(signature, &first);
        const auto position = signature->property("cursorPosition");
        for (int index = 0; index < 12; ++index) {
            auto row = backend.m_allAccounts[0].toMap(); row["unread_count"] = index; row["message_count"] = 1000 + index;
            backend.m_allAccounts = {row}; backend.publishAccounts(backend.m_allAccounts); emit backend.accountsChanged();
            frame();
            require(editor && editor->hasActiveFocus() && editor->property("cursorPosition") == position
                && editor->property("text").toString() == "Regards\nTest", "sync preserves signature editor, focus, cursor and unsaved text");
        }
        QInputMethodEvent next; next.setCommitString(" user"); QCoreApplication::sendEvent(editor, &next);
        require(editor->property("text").toString() == "Regards\nTest user", "typing continues after repeated sync refresh");
        settings->setVisible(false);
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(syncEngine.server.serverName());
        await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "scroll fixture connected");
        bool hello = false;
        backend.request("hello", {}, {{"token","synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
        await([&] { return hello; }, "scroll fixture authenticated");
        int pages = 0;
        syncEngine.customRequest = [&](auto *peer, const auto &request) {
            if (request["method"] == "conversations") {
                ++pages;
                require(request["params"].toObject()["live_paging"].toBool(), "native paging uses sync-safe cursors");
                if (pages == 1) {
                    QJsonArray rows;
                    for (int index = 500; index < 600; ++index) rows.append(QJsonObject::fromVariantMap(makeRow(index)));
                    peer->write(GoogleOperationFixture::reply(request["id"].toString(),
                        {{"conversations",rows},{"next_offset",301},{"next_cursor","next-page"}}));
                } else peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",request["id"]},{"ok",false},
                    {"error",QJsonObject{{"code","busy"},{"message","Synthetic retry required"}}}}).toJson(QJsonDocument::Compact)+'\n');
            } else peer->write(GoogleOperationFixture::reply(request["id"].toString(), {}));
            return true;
        };
        backend.m_connected = true; backend.m_shutdown = false;
        backend.m_nextOffset = 201; backend.m_nextCursor = "older-page";
        const auto scroll = list->property("contentHeight").toReal() - list->height() * 3;
        list->setProperty("contentY", scroll);
        await([&] { return pages == 1 && !backend.loading(); }, "scroll prefetches before the last viewport without Load more");
        frame();
        require(backend.m_conversations.size() == 301 && qAbs(list->property("contentY").toReal() - scroll) < 1,
            "prefetch appends unique rows without moving the visible scroll position");
        list->setProperty("contentY", list->property("contentHeight").toReal() - list->height() * 2);
        await([&] { return pages == 2 && !backend.loading(); }, "failed prefetch completes with retry state");
        require(!backend.listError().isEmpty(), "paging error is visible");
        backend.m_automaticReadScope = true;
        backend.loadConversations(false, true);
        backend.m_automaticReadScope = false;
        QElapsedTimer settled; settled.start();
        while (settled.elapsed() < 200) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
        require(pages == 2, "failure stops the Loading loop instead of immediately retrying");
        backend.loadMore(); await([&] { return pages == 3 && !backend.loading(); }, "explicit Retry can request another page");
        QQuickItem *scrollbar = nullptr;
        const auto findBar = [&](auto &&self, QQuickItem *item) -> void {
            if (item->objectName() == "persistentScrollBar" && item->property("orientation").toInt() == Qt::Vertical) scrollbar = item;
            for (auto *child : item->childItems()) self(self, child);
        };
        findBar(findBar, list);
        require(scrollbar && scrollbar->isVisible() && scrollbar->width() >= 14 && scrollbar->opacity() > 0.9,
            "mail scrollbar stays visible with a wide target while idle");
        const auto beforeClick = list->property("contentY").toReal();
        const auto point = scrollbar->mapToScene({scrollbar->width()/2, scrollbar->height()*0.2});
        const auto global = QPointF(window->mapToGlobal(point.toPoint()));
        QMouseEvent press(QEvent::MouseButtonPress,point,global,Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
        QMouseEvent release(QEvent::MouseButtonRelease,point,global,Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
        QCoreApplication::sendEvent(window,&press); QCoreApplication::sendEvent(window,&release);
        require(list->property("contentY").toReal() < beforeClick - list->height(), "visible scrollbar track jumps toward the clicked position");
        backend.m_shutdown = true;
        const QVariantMap source{{"account","one"},{"id","stable-message"},{"source_version","stable-version"},
            {"body_html","<p>Final formatted body with a <b>stable layout</b>.</p><blockquote>Older reply</blockquote>"},{"metadata",QVariantList{}}};
        const QVariantMap message{{"id","stable-message"},{"text","Plain alternative"},{"sender_name","Reader fixture"},{"sender_email","reader@example.test"},
            {"sent_at","2026-09-22T12:00:00Z"},{"unread",true},{"sent",false},{"source_available",true},{"source_version","stable-version"},
            {"reader_source",source},{"metadata",QVariantList{}},{"reader_sections",ReplyDocument::receivedBody(source.value("body_html").toString(),"Plain alternative")}};
        backend.m_selected = "row-13";
        backend.m_conversation = {{"id","row-13"},{"subject","Stable reader"},{"revision",1},{"messages",QVariantList{message}},
            {"reader_prepared",true},{"assistance_results",QVariantList{}}};
        emit backend.conversationChanged(); frame(); frame();
        QQuickItem *bodyItem = nullptr;
        const auto findBody = [&](auto &&self, QQuickItem *item) -> void {
            if (item->objectName() == "messageBody_stable-message") bodyItem = item;
            for (auto *child : item->childItems()) self(self, child);
        };
        findBody(findBody, window->contentItem()); require(bodyItem, "prepared body rendered in production reader");
        QPointer<QQuickItem> stableBody(bodyItem); const auto bodyHeight = bodyItem->height(); const auto bodyPosition = bodyItem->mapToScene({});
        for (int change = 0; change < 12; ++change) {
            auto flags = backend.m_conversation; auto messages = flags.value("messages").toList(); auto updated = messages.last().toMap();
            for (const auto key : {"reader_source","reader_sections","metadata"}) updated.remove(key);
            updated["unread"] = change % 2 == 0; messages.last() = updated; flags["messages"] = messages; flags["revision"] = change + 2;
            backend.m_conversation = backend.retainReaderPresentation(flags); emit backend.conversationChanged(); frame();
            require(stableBody && qAbs(stableBody->height()-bodyHeight)<0.5 && stableBody->mapToScene({}) == bodyPosition,
                "read receipts preserve the rendered body object, position and final height");
        }
        require(!warnings, "structural refresh and signature editing have no QML errors");
        qInfo() << "Actual MailView retained unique visible rows across 100-to-201 structural refresh.";
    }
    static void independentReader() {
        using GoogleOperationFixture::await;
        GoogleOperationFixture::Engine engine;
        MailBackend backend(engine.profile, "unused");
        backend.m_allAccounts = {account("one")}; backend.publishAccounts(backend.m_allAccounts);
        backend.m_account = "one"; backend.m_selected = "old";
        backend.m_enginePid = QCoreApplication::applicationPid();
        backend.m_mutatingConversation = true;
        bool statusHeld = false;
        engine.customRequest = [&](auto *peer, const auto &request) {
            const auto method = request["method"].toString();
            if (method == "categories") { statusHeld = true; return true; }
            if (method == "conversation") {
                peer->write(GoogleOperationFixture::reply(request["id"].toString(),
                    {{"id",request["params"].toObject()["id"]},{"revision",1},
                    {"messages",QJsonArray{QJsonObject{{"id","fresh"},{"text","Latest reader"},{"unread",false}}}}}));
                return true;
            }
            peer->write(GoogleOperationFixture::reply(request["id"].toString(), {})); return true;
        };
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(engine.server.serverName());
        await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "reader fixture connected");
        bool hello = false;
        backend.request("hello", {}, {{"token","synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
        await([&] { return hello; }, "reader fixture authenticated"); backend.m_connected = true;
        backend.request("categories", "one", {}, [](auto, bool) {});
        await([&] { return statusHeld; }, "background reply remains held");
        backend.openConversation("new");
        require(backend.m_selected == "new", "selection never waits for the previous read receipt");
        await([&] { return backend.m_conversation.value("id") == "new"; }, "new reader completes while old pipe stays blocked");
        require(statusHeld && !backend.m_inFlight.isEmpty() && backend.m_mutatingConversation,
            "reader bypasses held background query without changing mutation ownership");
    }
    static void readerFrames() {
        using GoogleOperationFixture::await;
        GoogleOperationFixture::Engine engine;
        MailBackend backend(engine.profile, "unused");
        backend.m_allAccounts = {account("one")}; backend.publishAccounts(backend.m_allAccounts);
        auto granted = backend.m_allAccounts[0].toMap();
        granted["google"] = QVariantMap{{"connection_state","connected"},{"capabilities",QVariantMap{{"mail_read",true},{"mail_write",true}}}};
        backend.m_allAccounts[0] = granted; backend.publishAccounts(backend.m_allAccounts);
        backend.m_account = "one"; backend.m_connected = true; backend.m_syntheticProfile = false;
        backend.m_enginePid = QCoreApplication::applicationPid();
        backend.m_preferences = {{"notifications","off"},{"theme","paper"},{"text_size","default"}};
        QVariantList rows;
        for (int i = 0; i < 1000; ++i) rows.append(QVariantMap{{"id",QString("thread-%1").arg(i)}, {"account","one"},
            {"subject",QString("Thread %1").arg(i)}, {"sender_name","Example sender"},{"sender_email","sender@example.test"},
            {"latest_at","2026-09-21T14:00:00Z"},{"group","done"},{"unread",false},{"starred",false},{"message_count",1}});
        backend.publishConversations(rows);
        int reads = 0, flags = 0;
        QStringList requested;
        const QJsonObject settings{{"revision",1},{"settings",QJsonObject{{"model",QJsonObject{{"name","Synthetic local model"}}},{"accounts",QJsonObject{}}}}};
        engine.customRequest = [&](auto *peer, const auto &request) {
            const auto method = request["method"].toString(); const auto params = request["params"].toObject();
            QJsonObject result;
            int delay = 0;
            if (method == "conversation") {
                ++reads; const auto id = params["id"].toString(); requested.append(id);
                result = {{"id",id},{"account","one"},{"subject","Prepared " + id},{"revision",1},
                    {"messages",QJsonArray{QJsonObject{{"id",id+"-message"},{"account","one"},{"text","Plain alternative"},
                        {"sent_at","2026-09-21T14:00:00Z"},{"sender_name","Example sender"},{"sender_email","sender@example.test"},
                        {"sent",false},{"kind","people"},{"unread",false},{"source_available",true},{"source_version","version"}}}}};
            } else if (method == "message_source") {
                auto html = QString("<p><b>Final body %1</b></p><p>Multiple lines of ordinary content with a useful <a href='https://example.test'>link</a>.</p>").arg(params["id"].toString());
                for (int i = 0; i < 24; ++i) html = "<table><tr><td>" + html + "</td></tr></table>";
                result = {{"id",params["id"]},{"account","one"},{"source_version","version"},{"body_html",html},
                    {"metadata",QJsonArray{QJsonObject{{"label","To"},{"people",QJsonArray{QJsonObject{{"email","me@example.test"},{"name","Me"}}}}}}}};
                delay = 25;
            } else if (method == "assist_settings_get") result = settings;
            else if (method == "assist_results") {
                const auto target = params["conversation_id"].toString()+"-message";
                result = {{"items",QJsonArray{QJsonObject{{"task","summary"},{"target",target},{"stale",false},{"partial",false},{"created_at","2026-09-21"},
                    {"output",QJsonObject{{"summary","A complete saved summary that wraps across several lines to expose any provisional reader geometry. This summary is already stored before the thread is opened."}}}}}}};
                // The old independent controller query can finish after the
                // fully prepared foreground reader. It must not replace it.
                if (request["id"] != "operation") { result = {{"items",QJsonArray{}}}; delay = 90; }
            } else if (method == "draft_get") result = {{"account","one"},{"conversation_id",params["conversation_id"]},{"revision",0},{"body",""}};
            else if (method == "flag") ++flags;
            const auto bytes = GoogleOperationFixture::reply(request["id"].toString(), result);
            QTimer::singleShot(delay, peer, [peer, bytes] { if (peer->state() == QLocalSocket::ConnectedState) peer->write(bytes); });
            return true;
        };
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(engine.server.serverName());
        await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "frame fixture connected");
        bool hello = false; backend.request("hello", {}, {{"token","synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
        await([&] { return hello; }, "frame fixture authenticated");
        for (const auto &weight : {"Regular", "Medium", "SemiBold"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf") >= 0, "reader uses the shipped font");
        QGuiApplication::setFont(QFont("Geist", 10));
        QTemporaryDir copy; const auto modulePath = copy.filePath("ReaderFrames"); require(QDir().mkpath(modulePath), "reader frame module");
        QByteArray module("module ReaderFrames\nsingleton Theme 1.0 Theme.qml\n");
        for (const auto &name : QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"}, QDir::Files)) {
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name), "unchanged actual reader QML");
            if(name=="ShortcutRegistry.qml") module+="singleton ";
            if(name!="Theme.qml") module+= name.chopped(4).toUtf8()+" 1.0 "+name.toUtf8()+'\n';
        }
        QFile registration(modulePath+"/qmldir"); require(registration.open(QIODevice::WriteOnly), "reader module registration"); registration.write(module); registration.close();
        qmlRegisterType<ReplyDocument>("ReaderFrames",1,0,"ReplyDocument");
        QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings = false;
        QObject::connect(&qml,&QQmlEngine::warnings,[&](const auto &errors){warnings=true;for(const auto &error:errors)qWarning()<<error.toString();});
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport ReaderFrames 1.0\nApplicationWindow { visible:true; width:1440; height:850; color: Theme.canvas; palette.text: Theme.text; palette.base: Theme.surface; palette.window: Theme.canvas; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Reader.qml")));
        require(!qml.rootObjects().isEmpty(), "reader frame window loads");
        auto *window = qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();}, "reader frame window exposed");
        const auto find = [&](auto &&self, QQuickItem *item, const QString &name) -> QQuickItem * {
            if (item->objectName() == name) return item;
            for (auto *child : item->childItems()) if (auto *match = self(self, child, name)) return match;
            return nullptr;
        };
        auto *list = find(find, window->contentItem(), "conversationList");
        auto *mail = find(find, window->contentItem(), "mailWorkspace");
        require(list && mail, "actual reader navigation controls");
        const auto frame = [&] {
            bool painted = false; auto c = QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{painted=true;});
            window->update(); await([&]{return painted;},"reader frame rendered"); QObject::disconnect(c);
        };
        // Observe every painted target frame, including its very first one.
        QList<QVariantList> geometry; QString observed = "thread-0";
        // Inspect geometry while Qt blocks the GUI for scene synchronization.
        // A queued frameSwapped handler can instead observe a later model update
        // that has not yet been polished/painted, producing a false first frame.
        const auto capture = QObject::connect(window,&QQuickWindow::afterSynchronizing,window,[&] {
            if (backend.m_conversation.value("id").toString() != observed) return;
            auto *body = find(find,window->contentItem(),"messageBody_"+observed+"-message");
            auto *summary = find(find,window->contentItem(),"messageSummary_"+observed+"-message");
            auto *reader = find(find,window->contentItem(),"reader");
            if (body && body->isVisible() && summary && reader) {
                require(body->property("text").toString().contains("Final body " + observed + "-message"), "first body frame uses the exact retained HTML source");
                require(summary->property("summaryText").toString().startsWith("A complete saved summary"), "first summary frame uses the prepared saved result");
                geometry.append({body->mapToScene({}).y(), body->height(), summary->height(), reader->property("contentHeight"), reader->property("contentY")});
            }
        }, Qt::DirectConnection);
        for (const auto &target : {QString("thread-0"),QString("thread-1"),QString("thread-0")}) {
            observed = target; geometry.clear();
            QElapsedTimer elapsed; elapsed.start(); backend.openConversation(target);
            await([&]{return !geometry.isEmpty();}, "first complete reader frame");
            const auto firstMs = elapsed.elapsed();
            // Provider acknowledgment can arrive after the cached reader paint.
            // Its status must not move the entire thread down and back up.
            for (const auto &state : {QString("pending"),QString("unknown"),QString("failed"),QString("acknowledged")}) {
                backend.m_mailDelivery[qMakePair(QString("one"), target)] = {{"state",state}};
                emit backend.mailDeliveryChanged();
                frame();
            }
            for (int i=0;i<12;++i) frame();
            for (const auto &entry : geometry) {
                if (entry != geometry.last()) qWarning()<<"Reader geometry changed"<<entry<<"to"<<geometry.last();
                require(entry == geometry.last(), "body/summary/scroll geometry is final on the first painted frame");
            }
            qInfo()<<"Reader frame"<<target<<"first paint"<<firstMs<<"ms; stable frames"<<geometry.size()<<geometry.last();
        }
        QObject::disconnect(capture);
        if (QGuiApplication::platformName() != "offscreen") {
            window->requestActivate();
            await([&]{return window->isActive();}, "native paging window receives keyboard focus");
        }
        list->forceActiveFocus();
        const auto key = [&](int code) { QKeyEvent event(QEvent::KeyPress,code,Qt::NoModifier); QCoreApplication::sendEvent(window,&event); };
        const auto captureDir = qEnvironmentVariable("FM_READER_CAPTURE_DIR");
        if (!captureDir.isEmpty()) {
            require(QDir().mkpath(captureDir), "reader capture directory");
            require(window->grabWindow().save(captureDir + "/reader.png"), "actual prepared reader capture");
        }
        const auto before = reads;
        QElapsedTimer burst; burst.start();
        for (int i=0;i<80;++i) key(Qt::Key_PageDown);
        key(Qt::Key_End);
        require(!mail->property("readerMatchesCursor").toBool(), "old reader actions are disabled while a new cursor target waits");
        key(Qt::Key_E); // Must not mark the previously displayed thread Done.
        for (int i=0;i<20;++i) key(Qt::Key_Up);
        const auto inputMs = burst.elapsed();
        require(inputMs<500, "queued paging burst cannot monopolize the UI");
        await([&]{return backend.m_conversation.value("id") == "thread-979";}, "burst opens exact accumulated final target");
        frame();
        require(reads-before <= 3, "one queued key burst starts bounded reader requests");
        require(flags==0, "skipped and read synthetic rows create no review mutations");
        qInfo()<<"102-key burst input"<<inputMs<<"ms; final painted reader"<<burst.elapsed()<<"ms; reader requests"<<reads-before<<"final target"<<backend.selectedId();
        for (int i=0;i<40;++i) { key(i%2 ? Qt::Key_PageUp : Qt::Key_PageDown); frame(); }
        key(Qt::Key_Home);
        await([&]{return backend.m_conversation.value("id") == "thread-0";}, "sustained alternating pages finish and Home stays responsive");
        frame(); require(!warnings,"production reader and paging have no QML warnings");
        backend.m_viewTimer.stop(); backend.m_readerPrefetchTimer.stop();
    }

    static void preparedReader() {
        using GoogleOperationFixture::await;
        GoogleOperationFixture::Engine engine;
        MailBackend backend(engine.profile, "unused");
        backend.m_allAccounts = {account("one")}; backend.publishAccounts(backend.m_allAccounts);
        backend.m_account = "one"; backend.m_selected = "slow"; backend.m_connected = true;
        backend.m_enginePid = QCoreApplication::applicationPid();
        int sourceReads = 0, flags = 0, publications = 0;
        QPointer<QLocalSocket> held; QString heldId;
        engine.customRequest = [&](auto *peer, const auto &request) {
            const auto method = request["method"].toString(), requestId = request["id"].toString();
            const auto params = request["params"].toObject();
            if (method == "conversation") {
                const auto id = params.value("id").toString();
                peer->write(GoogleOperationFixture::reply(requestId, {{"account","one"},{"id",id},{"revision",1},
                    {"messages",QJsonArray{QJsonObject{{"id",id+"-message"},{"text","Plain alternative"},{"source_available",true},{"source_version","version"},{"unread",true}}}}}));
            } else if (method == "message_source") {
                ++sourceReads;
                if (params.value("id") == "slow-message") { held = peer; heldId = requestId; return true; }
                const QJsonObject person{{"email","cc@example.test"},{"name","CC reader"}};
                const QJsonArray metadata{QJsonObject{{"label","Cc"},{"people",QJsonArray{person}}}};
                peer->write(GoogleOperationFixture::reply(requestId, {{"account","one"},{"id",params.value("id")},{"source_version","version"},
                    {"body_html","<p>Final <b>formatted</b> body</p><blockquote>Older quote</blockquote>"},{"metadata",metadata}}));
            } else if (method == "assist_results") {
                const QJsonObject summary{{"task","summary"},{"target",params.value("conversation_id").toString()+"-message"},{"output",QJsonObject{{"summary","Saved summary"}}}};
                peer->write(GoogleOperationFixture::reply(requestId, {{"items",QJsonArray{summary}}}));
            }
            else if (method == "assist_settings_get") peer->write(GoogleOperationFixture::reply(requestId, {{"revision",1},{"settings",QJsonObject{{"accounts",QJsonObject{}}}}}));
            else { if (method == "flag") ++flags; peer->write(GoogleOperationFixture::reply(requestId, {})); }
            return true;
        };
        QObject::connect(&backend, &MailBackend::conversationChanged, [&] {
            if (backend.m_conversation.isEmpty()) return;
            ++publications;
            const auto row = backend.m_conversation.value("messages").toList().last().toMap();
            require(row.contains("reader_sections") && row.contains("metadata")
                && backend.m_conversation.contains("assistance_results"), "first published reader includes formatting, metadata and saved summaries");
        });
        backend.loadConversation(false, false);
        await([&] { return !held.isNull(); }, "cold source read held");
        require(backend.m_conversation.isEmpty() && publications == 0, "plain alternative is not painted before the final source");
        backend.m_automaticReadScope = true; backend.loadConversation(false, false); backend.m_automaticReadScope = false;
        backend.loadConversation(false, true);
        require(sourceReads == 1 && backend.m_readerReviewPending && held->state() == QLocalSocket::ConnectedState,
            "polls and repeated clicks preserve pending source preparation and deliberate-open intent");
        ++backend.m_generation; backend.m_selected = "fast";
        backend.loadConversation(false, false);
        await([&] { return backend.m_conversation.value("id") == "fast"; }, "new reader completes while stale source is held");
        require(publications == 1 && flags == 0, "read-only preparation publishes once and does not mark mail reviewed");
        const auto currentReads = sourceReads;
        require(backend.requestMessageSource("slow-message", "version").isEmpty(), "retired reader member cannot request source in the new reader scope");
        QCoreApplication::processEvents();
        require(sourceReads == currentReads, "stale source controller makes no engine request");
        if (held && held->state() == QLocalSocket::ConnectedState) held->write(GoogleOperationFixture::reply(heldId, {{"account","one"},{"id","slow-message"},{"source_version","version"},{"body_html","Old response"}}));
        QCoreApplication::processEvents(); require(backend.m_conversation.value("id") == "fast", "late source cannot replace selected reader");
        backend.m_conversations = {QVariantMap{{"id","neighbor"},{"revision",1}}};
        backend.prefetchConversation("neighbor");
        await([&] { return backend.m_readerCache.object(qMakePair(QString("one"),QString("neighbor"))) != nullptr; }, "neighbor is fully prepared locally");
        const auto reads = sourceReads;
        require(backend.restoreConversation("one","neighbor") && backend.m_conversation.value("reader_prepared").toBool(), "warm navigation restores complete reader immediately");
        require(sourceReads == reads && flags == 0, "prefetch and cache restoration are read-only and need no additional source round trip");
        qInfo() << "Prepared reader passed: no provisional body, stale source ignored, one complete publication and read-only warm restore.";
    }
public:
    static void run() {
        readerFrames();
        preparedReader();
        independentReader();
        MailBackend backend("unused", "unused");
        backend.m_allAccounts = {account("one"),account("two")}; backend.publishAccounts(backend.m_allAccounts);
        backend.m_account = "one"; backend.m_selected = "thread";
        backend.m_views["one"] = QVariantMap{{"folder","inbox"},{"query",""},{"category",""},{"filters",QVariantMap{}},{"selected","thread"}};
        const auto original = reader("Preserve this rendered body", 3);
        backend.m_conversation = original;
        backend.cacheConversation("one","thread",original,1);
        backend.publishConversations({QVariantMap{{"id","thread"},{"account","one"},{"subject","Inbox item"}}});
        backend.m_nextOffset = 100; backend.m_nextCursor = "observed-page";
        backend.cacheMailView();
        backend.m_draft = {{"body","Unsent human text"}};
        backend.m_conversation.clear(); backend.m_conversations.clear(); backend.m_selected.clear();
        require(backend.restoreMailView(), "visited account restores before IPC");
        require(backend.m_selected == "thread" && backend.m_conversation == original && backend.m_conversations.size() == 1,
            "list and reader use the same observed account selection");
        require(backend.m_nextOffset == 100 && backend.m_nextCursor == "observed-page", "cached page keeps its cursor");
        require(backend.m_draft.value("body") == "Unsent human text", "cache restoration cannot overwrite authored text");
        require(backend.m_pending.isEmpty() && backend.m_outgoing.isEmpty(), "display restoration does not mark mail read or mutate anything");
        auto view = backend.m_views["one"].toMap(); view["query"] = "another search"; backend.m_views["one"] = view;
        require(!backend.restoreMailView(), "a different query cannot reuse the prior result list");
        view["query"] = ""; view["selectedDraft"] = "draft"; backend.m_views["one"] = view;
        require(!backend.restoreMailView(), "saved draft selection bypasses mail cache");
        backend.cacheConversation("one","thread",reader("stale response",2),1);
        require(backend.restoreConversation("one","thread") && backend.m_conversation == original, "late older response cannot regress the reader");
        backend.m_allAccounts = {account("one",2,"removed"),account("two")};
        require(!backend.restoreConversation("one","thread"), "removed account cannot restore active reader cache");
        backend.m_allAccounts = {account("one",3),account("two")};
        require(!backend.restoreConversation("one","thread"), "remove/restore invalidates old source lifetime");
        backend.cacheConversation("one","thread",reader("late before removal",99),1);
        require(!backend.restoreConversation("one","thread"), "late prior-lifetime completion cannot populate a restored account");

        int resets = 0, changes = 0;
        QObject::connect(&backend.m_messageModel, &QAbstractItemModel::modelReset, [&] { ++resets; });
        QObject::connect(&backend.m_messageModel, &QAbstractItemModel::dataChanged, [&] { ++changes; });
        backend.m_account = "one"; backend.m_selected = "thread"; backend.m_conversation = original; emit backend.conversationChanged();
        const int initialResets = resets;
        auto updated = original; auto messages = updated["messages"].toList(); auto message = messages[0].toMap();
        message["unread"] = false; messages[0] = message; updated["messages"] = messages; backend.m_conversation = updated; emit backend.conversationChanged();
        require(resets == initialResets && changes == 1 && backend.m_messageModel.rowCount() == 1,
            "read receipt updates the stable body delegate without resetting the reader model");
        emit backend.conversationChanged(); require(changes == 1 && resets == initialResets, "identical snapshots produce no model churn");
        backend.m_account = "two"; emit backend.conversationChanged();
        require(resets == initialResets + 1, "equal message IDs in another account are a different reader scope");

        const QString body(48 * 1024, QChar('x'));
        for (int index = 0; index < 200; ++index) backend.cacheConversation("two",QString::number(index),reader(body,index),1);
        require(backend.m_readerCache.totalCost() <= backend.m_readerCache.maxCost()
            && backend.m_readerCache.count() < 200 && backend.restoreConversation("two","199")
            && !backend.restoreConversation("two","0"), "reader cache evicts old bodies under its byte budget");
        qInfo() << "Mail cache passed: instant observed view, query/draft/lifecycle fences, no read mutation, stable body updates and bounded eviction.";
        visibleRefresh();
    }
};
int main(int argc, char **argv) { QGuiApplication app(argc,argv); QQuickStyle::setStyle("Basic"); MailNavigationCacheTest::run(); return 0; }
