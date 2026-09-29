// Reminder and new-mail notification test on a real MailBackend over the scripted
// engine: reminder receipts, permission and calendar-selection changes, Join,
// Dismiss and Snooze, the arrival cursor, and the ReminderPopup layout.
#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include <QDesktopServices>
#include <QGuiApplication>
#include <QJsonArray>
#include <QKeyEvent>
#include <QFontDatabase>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>

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
                backend.m_calendarSources = {source}; emit backend.calendarSourcesChanged();
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
    static void explicitSnooze() {
        Harness h;
        h.complete(h.prepareAck(),receipt(event()));
        require(h.backend.reminders().size()==1,"reminder shown before snooze");
        h.backend.snoozeCalendarReminder(event().toVariantMap());
        auto held=h.take("calendar_reminder_ack");
        require(held.request["params"].toObject()["state"]=="snoozed","snooze uses durable occurrence acknowledgement");
        h.complete(held,receipt(event(),"snoozed"));
        require(h.backend.reminders().isEmpty(),"snooze hides current reminder");
        h.poll(); h.complete(h.take("calendar_reminders"),{{"reminders",QJsonArray{}}});
        h.poll(); h.complete(h.take("calendar_reminders"),{{"reminders",QJsonArray{event()}}});
        auto resumed=h.take("calendar_reminder_ack");
        require(resumed.request["params"].toObject()["state"]=="shown","returned snooze can present again");
        h.complete(resumed,receipt(event()));
        require(h.backend.reminders().size()==1,"returned snooze is visible");
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
        for (const auto name : {"FmScrollBar.qml","Theme.qml","FmButton.qml","FmToolTip.qml","FmIcon.qml","ReminderPopup.qml","AttendeeList.qml","SenderAvatar.qml"})
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/" + name,copy.filePath(name)), "actual popup QML copied");
        QFile module(copy.filePath("qmldir")); require(module.open(QIODevice::WriteOnly), "popup singleton module");
        module.write("singleton Theme 1.0 Theme.qml\n"); module.close();
        for (const auto font : {"Geist-Regular.ttf","Geist-Medium.ttf"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/" + font) >= 0, "bundled popup font");
        auto current = event(); current["title"] = "<b>Human meeting title</b> — a longer invitation 日本語 that wraps without pushing either action outside the card";
        current["start"]=QDateTime::currentDateTimeUtc().addSecs(600).toString(Qt::ISODate);
        current["end"]=QDateTime::currentDateTimeUtc().addSecs(2400).toString(Qt::ISODate);
        h.backend.m_reminders = {current.toVariantMap()};
        QQmlApplicationEngine engine; FmIconItem::registerType(); bool warnings = false;
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
    Rectangle { id: backdrop; anchors.fill: parent; color: Theme.surface }
    TextArea { objectName: "typing"; width: 200; height: 60; text: "Hello there" }
    ReminderPopup { objectName: "reminderPopup"; anchors.fill: parent; backgroundSource: backdrop; visible: backend.reminders.length > 0 }
})QML",QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!engine.rootObjects().isEmpty(), "actual reminder popup loads");
        auto *root = engine.rootObjects().first(); auto *panel = root->findChild<QQuickItem *>("reminderPopup");
        require(panel != nullptr, "popup panel exists");
        const auto button = [&](const char *name) { return panel->findChild<QQuickItem *>(name); };
        await([&] { return button("reminderDismiss") && button("reminderPrimary") && button("reminderSnooze"); }, "modal actions created");
        for (const auto theme : {"graphite","paper","harbour"}) for (const auto size : {"default","large"}) {
            root->setProperty("appearance",theme); root->setProperty("textSize",size);
            // The action Flow re-lays out on the next polish, not synchronously.
            await([&] {
                const auto *dismiss = button("reminderDismiss"), *join = button("reminderPrimary"), *snooze=button("reminderSnooze");
                const auto a = dismiss->mapRectToItem(panel,dismiss->boundingRect()), b = join->mapRectToItem(panel,join->boundingRect()), c=snooze->mapRectToItem(panel,snooze->boundingRect());
                return panel->width()==430 && panel->height()==410 && a.width()>40 && b.width()>100 && c.width()>100 && !a.intersects(b) && !b.intersects(c);
            }, "short-window modal fills viewport and actions do not overlap");
            require(!warnings, "popup produces no QML binding/type warnings");
        }
        // The window comes forward once at one minute and once at the start,
        // muted or not, and not again when the same reminder is shown again.
        int summoned = 0;
        QObject::connect(&h.backend, &MailBackend::reminderAttentionRequested, [&] { ++summoned; });
        const auto startMs = panel->property("startMs").toDouble();
        const auto chimes = [&] { return panel->property("chimes").toInt(); };
        require(summoned == 0 && chimes() == 1, "a ten-minute reminder chimes once and leaves the window where it is");
        panel->setProperty("nowMs", startMs - 59000);
        require(summoned == 1 && chimes() == 2 && button("reminderKicker")->property("text") == "STARTING NOW", "one minute before sounds and brings the window forward");
        panel->setProperty("nowMs", startMs - 30000);
        require(summoned == 1 && chimes() == 2, "the rest of the last minute does not ask again");
        panel->setProperty("muted", true);
        panel->setProperty("nowMs", startMs + 1000);
        require(summoned == 2 && chimes() == 2, "the start brings a muted reminder forward without a sound");
        panel->setProperty("nowMs", startMs + 5000);
        require(summoned == 2, "a started reminder asks only once");
        panel->setProperty("muted", false);
        h.backend.m_reminders = {}; emit h.backend.remindersChanged();
        await([&] { return !panel->isVisible(); }, "refresh hides the reminder");
        h.backend.m_reminders = {current.toVariantMap()}; emit h.backend.remindersChanged();
        await([&] { return panel->isVisible(); }, "refresh shows the same reminder again");
        panel->setProperty("nowMs", startMs + 2000);
        require(summoned == 2 && chimes() == 2, "the same reminder shown again after its start stays in the background and silent");
        const auto capture=qEnvironmentVariable("FM_REMINDER_CAPTURE");
        if(!capture.isEmpty()) {
            auto window=qobject_cast<QQuickWindow *>(root);require(window,"reminder capture window");
            panel->setProperty("muted",true);root->setProperty("appearance","graphite");root->setProperty("textSize","default");
            window->resize(1440,900);
            const auto start=panel->property("startMs").toDouble();
            for(const auto remaining:{300,60,-10}) {
                panel->setProperty("nowMs",start-remaining*1000.0);
                QElapsedTimer rendered;rendered.start();while(rendered.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
                require(window->grabWindow().save(capture+QString::number(remaining)+".png"),"reminder state capture");
            }
        }
        // A reminder still open after its event ends is over: no Join, no Snooze, no alarm, only Close.
        panel->setProperty("nowMs",panel->property("endMs").toDouble()+1000);
        require(button("reminderKicker")->property("text")=="ENDED" && button("reminderCountdown")->property("text")=="Ended"
            && button("reminderRelative")->property("text")=="This event is over" && !panel->property("alerting").toBool()
            && !button("reminderPrimary")->isVisible() && !button("reminderSnooze")->isVisible()
            && button("reminderDismiss")->property("text")=="Close", "ended reminder offers only Close");
        require(QMetaObject::invokeMethod(button("reminderDismiss"),"clicked",Qt::DirectConnection), "actual QML dismissal handler");
        auto held = h.take("calendar_reminder_ack");
        require(held.request["params"].toObject()["occurrence_id"] == current["id"], "QML keeps the exact card occurrence");
        h.complete(held,receipt(current,"dismissed"));
        await([&] { return !panel->isVisible(); }, "dismissed popup closes");
        require(summoned == 2, "dismissing never brings the window forward");
        // A reminder that appears mid-typing ignores the keys already on their
        // way, and hands the cursor back where it was when it closes.
        auto *window = qobject_cast<QQuickWindow *>(root); auto *typing = root->findChild<QQuickItem *>("typing");
        require(window && typing, "typing window and field");
        window->requestActivate(); typing->forceActiveFocus(); typing->setProperty("cursorPosition", 5);
        require(typing->hasActiveFocus(), "the reply field has the cursor");
        h.backend.m_reminders = {current.toVariantMap()}; emit h.backend.remindersChanged();
        await([&] { return panel->isVisible() && panel->hasActiveFocus(); }, "the reminder takes the keyboard");
        const auto press = [&](int key, const QString &text) {
            QKeyEvent down(QEvent::KeyPress, key, Qt::NoModifier, text), up(QEvent::KeyRelease, key, Qt::NoModifier, text);
            QCoreApplication::sendEvent(window, &down); QCoreApplication::sendEvent(window, &up);
        };
        press(Qt::Key_S, "s"); press(Qt::Key_Return, "\r"); press(Qt::Key_Enter, "\r"); press(Qt::Key_Escape, "\x1b"); press(Qt::Key_M, "m");
        { QElapsedTimer settle; settle.start(); while (settle.elapsed() < 150) QCoreApplication::processEvents(QEventLoop::AllEvents, 10); }
        require(h.requests.isEmpty() && panel->isVisible() && !panel->property("muted").toBool() && typing->property("text") == "Hello there",
            "keys typed as the reminder appears neither snooze, join, dismiss, mute nor type into the hidden reply");
        await([&] { return !panel->property("keysGuarded").toBool(); }, "the key guard ends after a moment");
        press(Qt::Key_M, "m");
        require(panel->property("muted").toBool() && h.requests.isEmpty(), "the reminder's keys work again after the moment");
        require(QMetaObject::invokeMethod(button("reminderDismiss"),"clicked",Qt::DirectConnection), "dismiss by click");
        h.complete(h.take("calendar_reminder_ack"),receipt(current,"dismissed"));
        await([&] { return !panel->isVisible(); }, "the dismissed reminder closes");
        require(typing->hasActiveFocus() && typing->property("cursorPosition").toInt() == 5, "the cursor goes back to the reply where it was");
        require(!warnings, "popup dismissal leaves no binding errors");
    }
public:
    static void run() {
        shownAndRestart(); staleReplies(); mismatchedReceipts(); explicitJoin(); explicitSnooze(); mailCursor(); popup();
        qInfo() << "Notifications passed: private held IPC, exact receipts, current permissions/selection/lifetimes, restart suppression, explicit current Join and independent arrival cursor. No provider or OS notifications.";
    }
};

int main(int argc, char **argv) { QGuiApplication app(argc,argv); QQuickStyle::setStyle("Basic"); NotificationBackendTest::run(); return 0; }
#include "NotificationBackendTest.moc"
