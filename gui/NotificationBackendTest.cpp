#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include <QDesktopServices>
#include <QGuiApplication>
#include <QJsonArray>
#include <QFontDatabase>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>

using namespace GoogleOperationFixture;

class NotificationUrls : public QObject {
    Q_OBJECT
public:
    QList<QUrl> opened;
public slots:
    void open(const QUrl &url) { opened.append(url); }
};

class NotificationBackendTest {
    static QVariantMap account(int revision = 1) {
        return {{"id","account"}, {"lifecycle",QVariantMap{{"state","active"},{"revision",revision}}},
            {"google",QVariantMap{{"connection_state","connected"}, {"granted_scopes",QVariantList{"calendar.readonly"}}}}};
    }
    static QJsonObject event() {
        return {{"account","account"},{"calendar_id","calendar"},{"id","occurrence"},{"start","2026-09-14T13:00:00Z"},
            {"title","Permitted event"},{"account_name","Personal"},{"calendar_name","Work"},{"timing","Soon"},
            {"conference_url","https://meet.example.test/old"},{"reminder_state",QJsonValue::Null}};
    }
    static QJsonObject receipt(QJsonObject current, const QString &state = "shown") {
        return {{"account",current["account"]},{"calendar_id",current["calendar_id"]},{"occurrence_id",current["id"]},
            {"start",current["start"]},{"state",state},{"event",current},{"remote_data_changed",false}};
    }
    struct Held { QPointer<QLocalSocket> peer; QJsonObject request; };
    struct Harness {
        Engine engine;
        MailBackend backend{engine.profile,"unused",nullptr,false,true};
        QList<Held> requests;
        QList<QPair<QString,QString>> alerts;
        explicit Harness(const QString &mode = "events", bool navigating = false) {
            engine.customRequest = [this](auto *peer, const auto &request) {
                const auto method = request["method"].toString();
                require(method == "calendar_reminders" || method == "calendar_reminder_ack" || method == "mail_notifications", "only cached notification RPCs");
                requests.append({peer,request}); return true;
            };
            backend.m_allAccounts = {account()}; backend.publishAccounts(backend.m_allAccounts);
            backend.m_calendarSources = {QVariantMap{{"account","account"},{"calendar_id","calendar"},{"permission","owner"},{"revision",1},{"time_zone","UTC"}}};
            backend.m_preferences = {{"notifications",mode},{"calendar_display_time_zone","UTC"},{"selected_calendars",QVariant()}};
            QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
            backend.m_socket.connectToServer(engine.server.serverName());
            await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "notification private socket");
            bool hello = false;
            backend.request("hello", {}, {{"token","synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
            await([&] { return hello; }, "notification authenticated framing");
            QObject::connect(&backend, &MailBackend::desktopNotification, [&](const QString &title, const QString &body) { alerts.append({title,body}); });
            backend.m_connected = true; backend.m_syntheticProfile = false;
            backend.m_initialAccountsLoaded = true; backend.m_initialPreferencesLoaded = true;
            if (navigating) backend.m_queryActivity.start();
            backend.configureNotifications();
        }
        ~Harness() {
            backend.m_shutdown = true; backend.m_connected = false;
            backend.m_notificationTimer.stop(); backend.m_socket.abort();
        }
        Held take(const QString &method) {
            await([&] { return !requests.isEmpty(); }, "notification request arrives");
            auto held = requests.takeFirst();
            require(held.request["method"] == method, "exact notification method ordering");
            return held;
        }
        void complete(const Held &held, const QJsonObject &result, bool ok = true) {
            require(!held.peer.isNull(), "held cached connection remains owned");
            const auto id = held.request["id"].toString();
            if (ok) held.peer->write(reply(id,result));
            else held.peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",id},{"ok",false},
                {"error",QJsonObject{{"code","stale_reminder"},{"message","Permission changed"}}}}).toJson(QJsonDocument::Compact) + '\n');
            await([&] { return !backend.m_pending.contains(id); }, "held reply consumed by production callback");
        }
        void poll() { backend.m_queryActivity.invalidate(); backend.pollNotifications(); }
        Held prepareAck() {
            const auto read = take("calendar_reminders"); complete(read, {{"reminders",QJsonArray{event()}}});
            auto ack = take("calendar_reminder_ack");
            require(ack.request["account"] == "account" && ack.request["params"].toObject() == QJsonObject{
                {"calendar_id","calendar"},{"occurrence_id","occurrence"},{"start","2026-09-14T13:00:00Z"},{"state","shown"}}, "shown ACK retains the exact candidate tuple");
            return ack;
        }
        void fence(int kind) {
            backend.m_queryActivity.start();
            if (kind == 0) {
                auto removed = account(2); removed["lifecycle"] = QVariantMap{{"state","removed"},{"revision",2}};
                backend.m_allAccounts = {removed}; backend.publishAccounts({}); emit backend.accountsChanged();
                backend.m_allAccounts = {account(3)}; backend.publishAccounts(backend.m_allAccounts); emit backend.accountsChanged();
            } else if (kind == 1) {
                auto row = account(); auto google = row["google"].toMap(); google["granted_scopes"] = QVariantList{"calendar.freebusy"}; row["google"] = google;
                backend.m_allAccounts = {row}; backend.publishAccounts(backend.m_allAccounts); emit backend.accountsChanged();
            } else if (kind == 2) {
                backend.m_preferences["selected_calendars"] = QVariantList{}; emit backend.preferencesChanged();
            } else {
                auto source = backend.m_calendarSources[0].toMap(); source["permission"] = "free_busy"; source["revision"] = 2;
                backend.m_calendarSources = {source}; emit backend.eventsChanged();
            }
        }
    };
    static void shownAndRestart() {
        Harness h; const auto ack = h.prepareAck(); h.complete(ack,receipt(event()));
        require(h.alerts.size() == 1 && h.backend.reminders().size() == 1, "one alert only after durable shown ACK");
        h.poll(); auto shown = event(); shown["reminder_state"] = "shown";
        h.complete(h.take("calendar_reminders"),{{"reminders",QJsonArray{shown}}});
        require(h.alerts.size() == 1 && h.backend.reminders().size() == 1 && h.requests.isEmpty(), "shown current card is retained without duplicate ACK or desktop alert");
        h.poll(); h.complete(h.take("calendar_reminders"),{},false);
        require(h.backend.reminders().isEmpty(), "failed current permission projection clears stale details");
        Harness restarted;
        restarted.complete(restarted.take("calendar_reminders"),{{"reminders",QJsonArray{shown}}});
        require(restarted.alerts.isEmpty() && restarted.backend.reminders().isEmpty() && restarted.requests.isEmpty(), "durable shown receipt suppresses restart notification");
    }
    static void staleReplies() {
        for (int kind = 0; kind < 4; ++kind) {
            qInfo() << "Held notification fence" << kind;
            Harness read;
            const auto held = read.take("calendar_reminders"); read.fence(kind);
            read.complete(held,{{"reminders",QJsonArray{event()}}});
            require(read.alerts.isEmpty() && read.backend.reminders().isEmpty() && read.requests.isEmpty()
                && read.backend.m_pending.isEmpty(), "late projection cannot reacquire withdrawn authority");
            Harness ack;
            const auto pending = ack.prepareAck(); ack.fence(kind); ack.complete(pending,receipt(event()));
            require(ack.alerts.isEmpty() && ack.backend.reminders().isEmpty(), "late shown ACK cannot disclose stale account/permission/selection/source details");
            Harness shown;
            shown.complete(shown.prepareAck(),receipt(event())); shown.fence(kind);
            require(shown.backend.reminders().isEmpty(), "withdrawal removes an already visible title immediately");
        }
    }
    static void mismatchedReceipts() {
        for (const auto field : {"account","calendar_id","occurrence_id","start","state","event"}) {
            Harness h; auto wrong = receipt(event()); wrong[field] = "different";
            h.complete(h.prepareAck(),wrong);
            require(h.alerts.isEmpty() && h.backend.reminders().isEmpty(), "mismatched acknowledgment cannot show a card");
        }
    }
    static void explicitJoin() {
        NotificationUrls urls; QDesktopServices::setUrlHandler("https",&urls,"open");
        Harness h("off");
        auto future = event(); future["start"] = "2030-09-14T13:00:00Z";
        h.backend.joinCalendarEvent(future.toVariantMap()); auto held = h.take("calendar_reminder_ack");
        require(held.request["params"].toObject()["state"] == "dismissed", "explicit Join uses shared current-event acknowledgment");
        auto current = future; current["conference_url"] = "https://meet.example.test/current";
        h.complete(held,receipt(current,"dismissed"));
        require(urls.opened == QList<QUrl>{QUrl("https://meet.example.test/current")}, "Join works with alerts off and opens only current ACK URL");
        h.backend.joinCalendarEvent(future.toVariantMap()); held = h.take("calendar_reminder_ack"); h.fence(3);
        h.complete(held,receipt(current,"dismissed"));
        require(urls.opened.size() == 1, "held Join refuses a changed Calendar authority");
        h.backend.joinCalendarEvent(future.toVariantMap()); held = h.take("calendar_reminder_ack");
        auto wrong = receipt(current,"dismissed"); wrong["occurrence_id"] = "other"; h.complete(held,wrong);
        require(urls.opened.size() == 1, "Join cannot follow a different occurrence receipt");
        QDesktopServices::unsetUrlHandler("https");
    }
    static void mailCursor() {
        Harness h("all",true);
        auto first = h.take("mail_notifications");
        require(first.request["params"].toObject().isEmpty(), "startup samples MAX even during navigation, without reading historical messages");
        h.complete(first,{{"after",10},{"messages",QJsonArray{}},{"more",false}});
        require(h.requests.isEmpty() && h.alerts.isEmpty(), "Calendar idle deferral does not defer mail cursor initialization");
        const auto since = h.backend.m_notificationsSince;
        h.fence(2);
        require(h.backend.m_mailNotificationCursor == 10 && h.backend.m_notificationsSince == since, "Calendar selection does not reset mail arrival window");
        h.poll(); auto mail = h.take("mail_notifications");
        require(mail.request["params"].toObject()["after"] == 10 && !mail.request["params"].toObject()["since"].toString().isEmpty(), "next mail poll resumes the initialized cursor");
        const QJsonObject arrival{{"after",11},{"total",1},{"messages",QJsonArray{QJsonObject{{"account","account"},{"sender","Human"},{"subject","New arrival"}}}}};
        h.complete(mail,arrival); h.complete(h.take("calendar_reminders"),{},false);
        require(h.alerts == QList<QPair<QString,QString>>{{"Human","New arrival"}}, "mail arrival is independent of Calendar failure");
        h.poll(); mail = h.take("mail_notifications"); h.fence(0); h.complete(mail,arrival);
        require(h.alerts.size() == 1, "late mail alert cannot survive remove/restore");
        h.complete(h.take("calendar_reminders"),{{"reminders",QJsonArray{}}});
    }
    static void popup() {
        Harness h("off"); QTemporaryDir copy;
        require(copy.isValid(), "popup QML directory");
        for (const auto name : {"FmScrollBar.qml","Theme.qml","FmButton.qml","FmToolTip.qml","FmIcon.qml","ReminderPopup.qml"})
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/" + name,copy.filePath(name)), "actual popup QML copied");
        QFile module(copy.filePath("qmldir")); require(module.open(QIODevice::WriteOnly), "popup singleton module");
        module.write("singleton Theme 1.0 Theme.qml\n"); module.close();
        for (const auto font : {"Geist-Regular.ttf","Geist-Medium.ttf"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/" + font) >= 0, "bundled popup font");
        auto current = event(); current["title"] = "<b>Human meeting title</b> — a longer invitation 日本語 that wraps without pushing either action outside the card";
        h.backend.m_reminders = {current.toVariantMap()};
        QQmlApplicationEngine engine; bool warnings = false;
        QObject::connect(&engine,&QQmlEngine::warnings,[&](const auto &errors) {
            warnings = true; for (const auto &error : errors) qWarning() << error.toString();
        });
        engine.rootContext()->setContextProperty("backend",&h.backend);
        engine.loadData(R"QML(import QtQuick
import QtQuick.Controls
ApplicationWindow {
    width: 430; height: 410; visible: true
    property string appearance: "graphite"
    property string textSize: "default"
    Binding { target: Theme; property: "themeId"; value: appearance }
    Binding { target: Theme; property: "textSize"; value: textSize }
    ReminderPopup { objectName: "reminderPopup"; anchors.centerIn: parent }
})QML",QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!engine.rootObjects().isEmpty(), "actual reminder popup loads");
        auto *root = engine.rootObjects().first(); auto *panel = root->findChild<QQuickItem *>("reminderPopup");
        require(panel != nullptr, "popup panel exists");
        const auto button = [&](const QString &text) {
            QList<QQuickItem *> pending{panel};
            while (!pending.isEmpty()) {
                auto *item = pending.takeFirst(); pending.append(item->childItems());
                if (item->metaObject()->indexOfProperty("prominent") >= 0 && item->property("text") == text) return item;
            }
            return static_cast<QQuickItem *>(nullptr);
        };
        await([&] { return button("Dismiss") && button("Join"); }, "popup delegates created");
        for (const auto theme : {"graphite","paper","harbour"}) for (const auto size : {"default","large"}) {
            root->setProperty("appearance",theme); root->setProperty("textSize",size);
            QCoreApplication::processEvents(QEventLoop::AllEvents,20);
            const auto *dismiss = button("Dismiss"), *join = button("Join");
            const auto a = dismiss->mapRectToItem(panel,dismiss->boundingRect()), b = join->mapRectToItem(panel,join->boundingRect());
            require(a.width() > 40 && b.width() > 30 && a.left() >= 0 && b.right() <= panel->width()
                && a.bottom() <= panel->height() && b.bottom() <= panel->height() && !a.intersects(b), "default/large themed popup actions fit without overlap");
            require(!warnings, "popup produces no QML binding/type warnings");
        }
        require(QMetaObject::invokeMethod(button("Dismiss"),"clicked",Qt::DirectConnection), "actual QML dismissal handler");
        auto held = h.take("calendar_reminder_ack");
        require(held.request["params"].toObject()["occurrence_id"] == current["id"], "QML keeps the exact card occurrence");
        h.complete(held,receipt(current,"dismissed"));
        await([&] { return !panel->isVisible(); }, "dismissed popup closes");
        require(!warnings, "popup dismissal leaves no binding errors");
    }
public:
    static void run() {
        shownAndRestart(); staleReplies(); mismatchedReceipts(); explicitJoin(); mailCursor(); popup();
        qInfo() << "Notifications passed: private held IPC, exact receipts, current permissions/selection/lifetimes, restart suppression, explicit current Join and independent arrival cursor. No provider or OS notifications.";
    }
};

int main(int argc, char **argv) { QGuiApplication app(argc,argv); QQuickStyle::setStyle("Basic"); NotificationBackendTest::run(); return 0; }
#include "NotificationBackendTest.moc"
