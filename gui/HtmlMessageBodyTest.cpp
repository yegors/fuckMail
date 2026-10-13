#include "FmIconItem.h"
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QImage>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QTemporaryDir>
#include <QTest>
#include <QThread>
#include <QTimer>
#include <cstdio>
#include <functional>

// HtmlMessageBody, MessageNotices and RemoteContentSettings against a scripted
// service: image states, Load images, Always load images from, the
// proxy-unavailable row, blocked hosts, link activation, cancellation and the
// mailing-list row's unsubscribe states.
namespace {
int failures = 0;
void check(bool ok, const char *what) {
    std::fprintf(stderr, "%s: %s\n", ok ? "ok" : "FAIL", what);
    if (!ok) ++failures;
}
bool await(const std::function<bool()> &done, int ms = 8000) {
    QElapsedTimer timer; timer.start();
    while (!done() && timer.elapsed() < ms) { QCoreApplication::processEvents(QEventLoop::AllEvents, 10); QThread::msleep(2); }
    return done();
}
QQuickItem *find(QQuickItem *root, const QString &name) {
    if (root->objectName() == name) return root;
    for (auto child : root->childItems()) if (auto match = find(child, name)) return match;
    return nullptr;
}
}

class Service : public QObject {
    Q_OBJECT
    Q_PROPERTY(int revision READ revision NOTIFY revisionChanged)
    Q_PROPERTY(QVariantMap proxy READ proxy NOTIFY proxyChanged)
    Q_PROPERTY(QVariantMap imageCache READ imageCache NOTIFY imageCacheChanged)
public:
    QString red, blue;
    bool proxyDown = false, plainOnly = false;
    QStringList allowed, cancelled, requests;
    // Held results, with the images loaded since, as HtmlContent keeps them.
    QHash<QString, QVariantMap> results;
    QVariantMap savedProxy, proxyValue{{"enabled", false}, {"host", ""}, {"port", 443}, {"username", ""}, {"password_saved", false}};
    QString password;
    int tests = 0, serial = 0, rev = 0;
    int revision() const { return rev; }
    QVariantMap proxy() const { return proxyValue; }
    QVariantMap imageCache() const { return {{"bytes", 1024 * 1024}, {"limit", 256 * 1024 * 1024}}; }
    Q_INVOKABLE QString load(const QString &, const QString &id, const QString &version, bool quotes = false) {
        const auto ticket = QString("html-%1").arg(++serial);
        QTimer::singleShot(0, this, [this, ticket, id, version, quotes] {
            QVariantMap result{{"id", id}, {"source_version", version}, {"has_html", !plainOnly}, {"too_large", false}};
            if (!plainOnly) {
                result["html"] = "<!DOCTYPE html><html><body style=\"margin:8px\"><p><a href=\"fm-link:1\">News</a> <a href=\"fm-link:2\">Ad</a></p>"
                                 "<img src=\"fm-res:1\" width=\"200\" height=\"80\"><img src=\"fm-res:2\" width=\"40\" height=\"40\">"
                                 "<img src=\"fm-res:3\" width=\"60\" height=\"40\"><img src=\"fm-res:4\" width=\"60\" height=\"40\">"
                                 + QString(quotes ? "<blockquote><p>Quoted answer</p></blockquote>" : "") + "</body></html>";
                result["quote_removed"] = !quotes;
                result["resources"] = QVariantList{
                    QVariantMap{{"id", 1}, {"kind", "remote"}, {"alt", "Hero"}, {"host", "img.test"}},
                    QVariantMap{{"id", 2}, {"kind", "cid"}, {"alt", "Logo"}},
                    QVariantMap{{"id", 3}, {"kind", "remote"}, {"alt", "Pixel"}, {"host", "tracker.io"}},
                    QVariantMap{{"id", 4}, {"kind", "insecure"}, {"alt", "Old"}, {"host", "old.test"}}};
                result["links"] = QVariantList{
                    QVariantMap{{"id", 1}, {"href", "https://news.test/a"}, {"scheme", "https"}, {"host", "news.test"}},
                    QVariantMap{{"id", 2}, {"href", "https://tracker.io/c"}, {"scheme", "https"}, {"host", "tracker.io"}}};
            }
            results.insert(id + "/" + version + (quotes ? "/quotes" : ""), result);
            emit htmlReady(ticket, result, true);
        });
        return ticket;
    }
    Q_INVOKABLE QVariantMap cached(const QString &, const QString &id, const QString &version, bool quotes = false) const { return results.value(id + "/" + version + (quotes ? "/quotes" : "")); }
    Q_INVOKABLE QString loadImage(const QString &, const QString &id, const QString &version, int resource, bool cachedOnly, bool quotes = false) {
        const auto ticket = QString("image-%1").arg(++serial);
        requests.append(QString("%1:%2%3%4").arg(id).arg(resource).arg(cachedOnly ? "?" : "").arg(quotes ? "q" : ""));
        QTimer::singleShot(0, this, [this, ticket, resource, key = id + "/" + version + (quotes ? "/quotes" : "")] {
            if (resource == 1 && proxyDown) { emit imageReady(ticket, {{"state", "proxy_unavailable"}, {"reason", "proxy:connect"}}, true); return; }
            const QVariantMap image{{"path", resource == 2 ? red : blue}, {"width", 40}, {"height", 40}};
            if (results.contains(key)) {
                auto resources = results[key]["resources"].toList();
                for (auto &entry : resources) {
                    auto value = entry.toMap();
                    if (value["id"].toInt() == resource) { value["image"] = image; entry = value; }
                }
                results[key]["resources"] = resources;
            }
            auto result = image;
            result["state"] = "ready";
            emit imageReady(ticket, result, true);
        });
        return ticket;
    }
    Q_INVOKABLE void cancelMessage(const QString &account, const QString &id) { cancelled.append(account + "/" + id); }
    Q_INVOKABLE QString blockedBy(const QString &host) const { return host == "tracker.io" ? host : QString(); }
    Q_INVOKABLE bool imagesAllowed(const QString &account, const QString &email) const { return allowed.contains(account + "/" + email); }
    Q_INVOKABLE void allowImages(const QString &account, const QString &email) { allowed.append(account + "/" + email); }
    Q_INVOKABLE void disallowImages(const QString &account, const QString &email) { allowed.removeAll(account + "/" + email); }
    Q_INVOKABLE QString saveProxy(bool enabled, const QString &host, const QString &username) {
        if (host.contains("://")) return "Enter the proxy's host name, without https:// or a path";
        savedProxy = {{"enabled", enabled}, {"host", host}, {"username", username}};
        proxyValue["enabled"] = enabled; proxyValue["host"] = host; proxyValue["username"] = username;
        emit proxyChanged();
        return {};
    }
    Q_INVOKABLE void setProxyPassword(const QString &value) { password = value; proxyValue["password_saved"] = true; emit proxyChanged(); }
    Q_INVOKABLE void clearProxyPassword() { password.clear(); proxyValue["password_saved"] = false; emit proxyChanged(); }
    Q_INVOKABLE void refreshProxy() {}
    Q_INVOKABLE void refreshImageCache() {}
    Q_INVOKABLE void clearImageCache() {}
    Q_INVOKABLE void testProxy() { ++tests; proxyValue["test"] = QVariantMap{{"ok", true}, {"ms", 42}}; emit proxyChanged(); }
signals:
    void htmlReady(const QString &ticket, const QVariantMap &result, bool ok);
    void imageReady(const QString &ticket, const QVariantMap &result, bool ok);
    void blockedHostsChanged();
    void revisionChanged();
    void proxyChanged();
    void imageCacheChanged();
};

class Backend : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(QString account READ account CONSTANT)
    Q_PROPERTY(QString preferencesStatus READ preferencesStatus CONSTANT)
    Q_PROPERTY(QObject *htmlContent READ htmlContent CONSTANT)
    Q_PROPERTY(QVariantList events READ events NOTIFY eventsChanged)
    Q_PROPERTY(bool syntheticProfile READ syntheticProfile CONSTANT)
public:
    Service service;
    QStringList opened;
    bool connected() const { return true; }
    QString account() const { return "acct"; }
    QString preferencesStatus() const { return {}; }
    QObject *htmlContent() { return &service; }
    QVariantList unsubscribes;
    Q_INVOKABLE void openLink(const QString &url) { opened.append(url); }
    Q_INVOKABLE QString unsubscribe(const QVariantMap &message) { unsubscribes.append(message); return "u" + QString::number(unsubscribes.size()); }
    // The invitation row's calendar calls.
    QVariantList events() const { return {}; }
    bool syntheticProfile() const { return false; }
    QStringList invites;
    QVariantList writes, declines;
    bool declining = false;
    Q_INVOKABLE QString requestMessageInvite(const QString &id, const QString &) { invites.append(id); return "i" + QString::number(invites.size()); }
    Q_INVOKABLE QString inviteWhen(const QString &start, const QString &end, bool withEnd = true) const { return withEnd ? start + " to " + end : start; }
    Q_INVOKABLE bool calendarEventDeclining(const QVariantMap &) const { return declining; }
    Q_INVOKABLE void calendarWrite(const QVariantMap &values) { writes.append(values); }
    Q_INVOKABLE QString queueCalendarDecline(const QVariantMap &values, const QVariantMap &) { declines.append(values); declining = true; emit eventsChanged(); return {}; }
signals:
    void connectedChanged();
    void unsubscribeReady(const QString &ticket, const QVariantMap &result, bool ok);
    void messageInviteReady(const QString &ticket, const QVariantMap &result, bool ok);
    void calendarWriteFinished(bool ok, const QVariantMap &result, const QString &error);
    void eventsChanged();
};

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QQuickStyle::setStyle("Basic");
    for (const auto file : {"Geist-Regular.ttf", "Geist-Medium.ttf"}) QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR) + "/fonts/" + file);
    QTemporaryDir temp;
    const auto dir = temp.filePath("HtmlTest");
    QDir().mkpath(dir);
    for (const auto name : {"Theme.qml", "FmIcon.qml", "FmButton.qml", "FmToolTip.qml", "FmSwitch.qml", "FmTextField.qml","FmMenu.qml","FmMenuItem.qml","TextMenu.qml","TextMenuHandler.qml", "HtmlMessageBody.qml", "MessageNotices.qml", "RemoteContentSettings.qml"})
        check(QFile::copy(QString(FM_GUI_SOURCE_DIR) + "/" + name, dir + "/" + name), "copy production QML");
    QFile module(dir + "/qmldir");
    check(module.open(QIODevice::WriteOnly), "module metadata");
    module.write("module HtmlTest\nsingleton Theme 1.0 Theme.qml\nHtmlMessageBody 1.0 HtmlMessageBody.qml\nMessageNotices 1.0 MessageNotices.qml\nRemoteContentSettings 1.0 RemoteContentSettings.qml\n");
    module.close();
    Backend backend;
    QImage image(40, 40, QImage::Format_ARGB32);
    image.fill(Qt::red); backend.service.red = temp.filePath("red.png"); image.save(backend.service.red);
    image.fill(Qt::blue); backend.service.blue = temp.filePath("blue.png"); image.save(backend.service.blue);

    QQmlApplicationEngine qml;
    FmIconItem::registerType();
    bool warnings = false;
    QObject::connect(&qml, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { warnings = true; for (const auto &e : errors) std::fprintf(stderr, "%s\n", qPrintable(e.toString())); });
    qml.addImportPath(temp.path());
    qml.rootContext()->setContextProperty("backend", &backend);
    qml.loadData(R"(
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import HtmlTest 1.0
ApplicationWindow {
    id: root
    width: 600; height: 1000; visible: true; color: Theme.canvas
    property var message: ({id: "m1", source_available: true, source_version: "v1", sender_email: "news@x.test", sender_name: "News"})
    property bool shown: true
    property string hovered: ""
    property var mailingList: null
    property var unsubscribe: null
    property real noticesWidth: 560
    property string openedInvite: ""
    function show(next) { shown = false; message = next; shown = true }
    ColumnLayout {
        x: 20; y: 20; width: root.noticesWidth; spacing: 0
        MessageNotices {
            objectName: "notices"; Layout.fillWidth: true; Layout.bottomMargin: visible ? 9 : 0
            message: root.message; mailingList: root.mailingList; unsubscribe: root.unsubscribe
            onOpenInvite: function(event, declined) { root.openedInvite = event.event_id + (declined ? " declined" : "") }
            htmlBody: loader.item && !loader.item.failed ? loader.item : null
        }
        Loader {
            id: loader; Layout.fillWidth: true; Layout.preferredHeight: item ? item.implicitHeight : 0; active: shown
            sourceComponent: HtmlMessageBody {
                objectName: "body"; width: 560; message: root.message; account: "acct"; active: true
                onLinkHovered: function(href, box) { root.hovered = href + "@" + Math.round(box.width) }
                onLinkExited: root.hovered = ""
            }
        }
    }
    RemoteContentSettings { objectName: "settings"; x: 20; y: 480; width: 560 }
}
)");
    auto *window = qobject_cast<QQuickWindow *>(qml.rootObjects().value(0));
    check(window, "the test window loads");
    if (!window) return 1;
    check(QTest::qWaitForWindowExposed(window), "the window is exposed");
    auto &service = backend.service;
    auto body = [&] { return find(window->contentItem(), "body"); };
    auto view = [&] { return find(window->contentItem(), "messageBody_" + window->property("message").toMap()["id"].toString()); };
    const auto ready = [&] { return body() && body()->property("ready").toBool(); };
    const auto show = [&](const QString &id, const QString &sender) {
        const QVariant next = QVariantMap{{"id", id}, {"source_available", true}, {"source_version", "v-" + id}, {"sender_email", sender}, {"sender_name", "Other"}};
        QMetaObject::invokeMethod(window, "show", Q_ARG(QVariant, next));
    };

    check(await(ready), "the message lays out on its paper card");
    // Search matches are painted on the paper like a selection.
    body()->setProperty("matchPhrases", QStringList{"news"});
    check(await([&] { return view()->property("highlightCount").toInt() == 1; }), "a search match in the HTML body is tinted");
    body()->setProperty("matchPhrases", QStringList{});
    check(await([&] { return view()->property("highlightCount").toInt() == 0; }), "clearing the search clears the tint");
    check(await([&] { return service.requests == QStringList{"m1:2"}; }), "only the inline image is requested without asking");
    auto images = body()->property("images").toMap();
    check(images["1"].toMap()["state"] == "hidden" && images["3"].toMap()["state"] == "blocked" && images["4"].toMap()["state"] == "insecure",
          "remote images wait, blocked hosts and http images never load");
    check(await([&] { return body()->property("images").toMap()["2"].toMap()["state"] == "ready"; }), "the inline image shows");
    check(body()->property("hiddenCount").toInt() == 1 && find(window->contentItem(), "htmlImageBar_m1")->isVisible(), "the images row counts hidden images");
    check(!find(window->contentItem(), "unsubscribeRow_m1")->isVisible(), "without a mailing list only the images row shows");
    {
        auto *row = find(window->contentItem(), "htmlImageBar_m1");
        QString text;
        for (auto child : row->childItems()) if (child->property("text").toString().contains("hidden")) text = child->property("text").toString();
        check(text == QString::fromUtf8("1 image hidden so News can\u2019t tell you opened this"), "the images row names the sender");
        check(find(window->contentItem(), "htmlAllowImages_m1")->property("text") == "Always load images from News", "Always load images names the sender");
    }
    check(body()->property("blockedLinks").toList() == QVariantList{2}, "links to blocked hosts are struck through");

    // Load images fetches the waiting remote image, never the blocked one.
    auto *load = find(window->contentItem(), "htmlLoadImages_m1");
    // The bar appears as the paper grows; click once the column has moved the paper below it.
    auto *bar = find(window->contentItem(), "htmlImageBar_m1");
    check(await([&] {
        const auto barRect = bar->mapRectToScene(bar->boundingRect()), viewRect = view()->mapRectToScene(view()->boundingRect());
        return load->width() > 0 && viewRect.top() >= barRect.bottom();
    }), "the image bar lays out above the paper");
    // Buttons keep their full label inside the row; the row's text gives way first.
    auto inside = [&](QQuickItem *item, const char *rowName) {
        auto *row = find(window->contentItem(), rowName);
        const auto a = item->mapRectToScene(item->boundingRect()), b = row->mapRectToScene(row->boundingRect());
        return item->isVisible() && a.left() >= b.left() - 0.5 && a.right() <= b.right() + 0.5 && item->width() + 0.5 >= item->implicitWidth();
    };
    check(inside(load, "htmlImageBar_m1") && inside(find(window->contentItem(), "htmlAllowImages_m1"), "htmlImageBar_m1"), "Load images and Always load images fit in their row");
    QTest::mouseMove(window, load->mapToScene(QPointF(load->width() / 2, load->height() / 2)).toPoint());
    QTest::mouseClick(window, Qt::LeftButton, {}, load->mapToScene(QPointF(load->width() / 2, load->height() / 2)).toPoint());
    check(await([&] { return body()->property("images").toMap()["1"].toMap()["state"] == "ready"; }), "Load images loads the remote image");
    check(!service.requests.join(",").contains(":3") && !service.requests.join(",").contains(":4"), "Load images skips blocked and http images");
    check(await([&] { return find(window->contentItem(), "htmlImagesLoaded_m1")->isVisible(); }), "Load images becomes a loaded count");
    check(!load->isVisible(), "the Load images button gives way to the count");
    check(find(window->contentItem(), "htmlImageText_m1")->property("text") == "Images loaded for this message only", "the row says the images are for this message only");
    check(find(window->contentItem(), "htmlAllowImages_m1")->isVisible()
          && find(window->contentItem(), "htmlAllowImages_m1")->property("text") == "Always load images from News", "Always load images stays after loading");
    {
        QString loaded;
        for (auto child : find(window->contentItem(), "htmlImagesLoaded_m1")->childItems())
            if (child->property("text").toString().contains("loaded")) loaded = child->property("text").toString();
        check(loaded == "1 image loaded", "the count names the loaded images");
    }

    // The mailing-list row: counts, the method's button, then the done state.
    window->setProperty("mailingList", QVariantMap{{"month", 3}, {"year", 12}, {"unsubscribed", QVariant()}});
    window->setProperty("unsubscribe", QVariantMap{{"method", "one_click"}, {"url", "https://news.test/u"}});
    auto *unsubscribeText = find(window->contentItem(), "unsubscribeText_m1");
    auto *unsubscribeButton = find(window->contentItem(), "unsubscribeButton_m1");
    check(await([&] { return find(window->contentItem(), "unsubscribeRow_m1")->isVisible() && unsubscribeButton->width() > 0; }), "a list message shows the mailing-list row");
    check(unsubscribeText->property("text") == QString::fromUtf8("Mailing list \u00b7 3 this month, 12 this year"), "the row counts the sender's mail");
    check(inside(unsubscribeButton, "unsubscribeRow_m1"), "Unsubscribe fits inside its row");
    if (const auto shot = qEnvironmentVariable("FM_NOTICES_CAPTURE"); !shot.isEmpty()) window->grabWindow().save(shot);
    // A narrow reader squeezes the text, never the buttons.
    window->setProperty("noticesWidth", 380);
    check(await([&] { return inside(unsubscribeButton, "unsubscribeRow_m1") && inside(find(window->contentItem(), "htmlAllowImages_m1"), "htmlImageBar_m1"); }), "buttons stay whole in a narrow reader");
    window->setProperty("noticesWidth", 560);
    QTest::mouseClick(window, Qt::LeftButton, {}, unsubscribeButton->mapToScene(QPointF(unsubscribeButton->width() / 2, unsubscribeButton->height() / 2)).toPoint());
    check(backend.unsubscribes.size() == 1 && backend.unsubscribes.value(0).toMap()["method"] == "one_click" && backend.unsubscribes.value(0).toMap()["id"] == "m1", "Unsubscribe asks the backend with the message's method");
    check(unsubscribeButton->property("text").toString() == QString("Unsubscribing") + QChar(0x2026), "the button shows progress");
    emit backend.unsubscribeReady("u1", {{"method", "one_click"}}, true);
    window->setProperty("mailingList", QVariantMap{{"month", 3}, {"year", 12}, {"unsubscribed", QVariantMap{{"method", "one_click"}}}});
    check(await([&] { return !unsubscribeButton->isVisible(); }), "the button hides once unsubscribed");
    check(unsubscribeText->property("text") == QString::fromUtf8("Unsubscribed from News \u00b7 it can take a few days to stop"), "the row confirms the unsubscribe");
    window->setProperty("mailingList", QVariantMap{{"month", 3}, {"year", 12}, {"unsubscribed", QVariantMap{{"method", "link"}}}});
    check(unsubscribeText->property("text") == "Unsubscribe page opened in your browser", "a link reads as opened in the browser");
    // The invitation row comes first: its time, the time it moved from, your
    // answer once given, and Yes / Maybe / No updating the same event.
    window->setProperty("mailingList", QVariantMap{{"month", 3}, {"year", 12}, {"unsubscribed", QVariant()}});
    auto *notices = find(window->contentItem(), "notices");
    notices->setProperty("hasInvite", true);
    check(await([&] { return backend.invites == QStringList{"m1"}; }), "an invitation asks for its event");
    const QVariantList people{QVariantMap{{"email", "me@x.test"}, {"self", true}, {"responseStatus", "needsAction"}},
        QVariantMap{{"email", "rebecca@x.test"}, {"organizer", true}, {"responseStatus", "accepted"}}};
    QVariantMap event{{"id", "acct:primary:ev"}, {"account", "acct"}, {"calendar_id", "primary"}, {"event_id", "ev"}, {"etag", "e1"},
        {"start", "2026-09-29T19:00:00Z"}, {"end", "2026-09-29T20:00:00Z"}, {"date", "2026-09-29"}, {"can_rsvp", true},
        {"organizer", "rebecca@x.test"}, {"attendees", people}};
    const auto answerWith = [&](const QVariantMap &value) {
        return QVariantMap{{"invite", QVariantMap{{"method", "REQUEST"}, {"start", value["start"]}, {"end", value["end"]},
            {"was_start", "2026-09-25T19:30:00Z"}, {"was_end", "2026-09-25T20:30:00Z"}, {"event", value}}}};
    };
    emit backend.messageInviteReady("i1", answerWith(event), true);
    auto *inviteRow = find(window->contentItem(), "inviteRow_m1");
    check(await([&] { return inviteRow->isVisible(); }), "the invitation row shows");
    check(await([&] { auto *list = find(window->contentItem(), "unsubscribeRow_m1"); return list->isVisible() && inviteRow->mapToScene({0, 0}).y() + inviteRow->height() <= list->mapToScene({0, 0}).y(); }), "the invitation comes before the mailing list");
    check(find(window->contentItem(), "inviteWas_m1")->property("text") == "was 2026-09-25T19:30:00Z", "an update names the time it moved from");
    auto *state = find(window->contentItem(), "inviteState_m1");
    auto *yes = find(window->contentItem(), "inviteAnswer-yes"), *no = find(window->contentItem(), "inviteAnswer-no");
    check(!state->isVisible() && !yes->property("chosen").toBool() && !no->property("chosen").toBool(), "unanswered shows no status and nothing chosen");
    QMetaObject::invokeMethod(yes, "clicked");
    check(backend.writes.size() == 1 && backend.writes[0].toMap()["response"] == "accepted" && backend.writes[0].toMap()["event_id"] == "ev"
        && backend.writes[0].toMap()["expected_etag"] == "e1", "Yes answers the linked event");
    check(state->property("text") == "Going" && yes->property("chosen").toBool(), "the answer shows at once");
    emit backend.calendarWriteFinished(true, {{"account", "acct"}, {"request_id", backend.writes[0].toMap()["request_id"]}}, QString());
    check(await([&] { return backend.invites.size() == 2; }), "a finished answer reads the event again");
    auto accepted = event; accepted["self_response"] = "accepted";
    emit backend.messageInviteReady("i2", answerWith(accepted), true);
    check(state->property("text") == "Going" && yes->property("chosen").toBool(), "the calendar's answer keeps it");
    QMetaObject::invokeMethod(no, "clicked");
    check(backend.declines.size() == 1 && backend.declines[0].toMap()["response"] == "declined", "No declines through the Undo hold");
    check(await([&] { return state->property("text") == "Declined" && no->property("chosen").toBool(); }), "a decline shows at once");
    const auto when = find(window->contentItem(), "inviteWhen_m1");
    QTest::mouseClick(window, Qt::LeftButton, {}, when->mapToScene(QPointF(10, when->height() / 2)).toPoint());
    check(window->property("openedInvite") == "ev declined", "the time opens the event, asking for declined events to show");
    notices->setProperty("hasInvite", false);
    backend.declining = false;
    window->setProperty("mailingList", QVariant());
    window->setProperty("unsubscribe", QVariant());

    // Links open through the backend unless their host is blocked.
    QMetaObject::invokeMethod(view(), "linkActivated", Q_ARG(int, 1));
    QMetaObject::invokeMethod(view(), "linkActivated", Q_ARG(int, 2));
    check(backend.opened == QStringList{"https://news.test/a"}, "a blocked link does not open");
    const auto box = view()->property("hoveredLink");
    Q_UNUSED(box)
    const auto rects = [&] { QVariantList value; QMetaObject::invokeMethod(view(), "linkRects", Q_RETURN_ARG(QVariantList, value), Q_ARG(int, 1)); return value; }();
    check(!rects.isEmpty(), "the view publishes the link box");
    if (!rects.isEmpty()) {
        QTest::mouseMove(window, view()->mapToScene(rects.first().toRectF().center()).toPoint());
        check(await([&] { return window->property("hovered").toString().startsWith("https://news.test/a@"); }), "hovering a link reports its href and box");
    }

    // An unavailable proxy never falls back; the bar offers Retry.
    service.proxyDown = true;
    show("m2", "other@x.test");
    check(await(ready) && service.cancelled.contains("acct/m1"), "leaving a message cancels its image work");
    QMetaObject::invokeMethod(body(), "loadImages");
    check(await([&] { return body()->property("proxyFailed").toBool() && find(window->contentItem(), "htmlImageBar_m2")->isVisible(); }), "a failed proxy shows the unavailable bar");
    check(find(window->contentItem(), "htmlLoadImages_m2")->property("text") == "Retry", "the bar offers Retry");
    service.proxyDown = false;

    // Always from remembers the sender for this account.
    QMetaObject::invokeMethod(body(), "allowSender");
    check(service.allowed == QStringList{"acct/other@x.test"}, "Always from saves the sender");
    check(await([&] { return body()->property("images").toMap()["1"].toMap()["state"] == "ready"; }), "Always from also loads this message");
    check(await([&] { return find(window->contentItem(), "htmlImageText_m2")->property("text") == "Images from Other load automatically"; }), "the row says the sender's images load automatically");
    check(find(window->contentItem(), "htmlAllowImages_m2")->property("text") == "Stop always loading from Other", "the button offers to stop");
    QMetaObject::invokeMethod(body(), "disallowSender");
    check(service.allowed.isEmpty(), "Stop always loading forgets the sender");
    check(body()->property("images").toMap()["1"].toMap()["state"] == "ready", "images already loaded stay loaded");
    check(find(window->contentItem(), "htmlImageText_m2")->property("text") == "Images loaded for this message only"
          && find(window->contentItem(), "htmlAllowImages_m2")->property("text") == "Always load images from Other", "the row returns to this message only");
    QMetaObject::invokeMethod(body(), "allowSender");
    const auto before = service.requests.size();
    show("m3", "other@x.test");
    check(await([&] { return ready() && service.requests.mid(before).contains("m3:1"); }), "an allowed sender's next message loads remote images");

    // A message shown again uses the held result and has its final height on its first frame.
    const auto paper = [&] { return view()->parentItem(); };
    const auto settledImages = [&] {
        const auto images = body()->property("images").toMap();
        for (const auto &value : images) if (value.toMap()["state"] == "loading") return false;
        return !images.isEmpty();
    };
    check(await([&] { return ready() && settledImages() && view()->property("implicitHeight").toReal() > 0; }), "the message settles");
    QTest::qWait(200);
    const qreal settled = paper()->height();
    const int requestsBefore = service.requests.size();
    qreal firstFrame = -1;
    auto frames = QObject::connect(window, &QQuickWindow::frameSwapped, window, [&] { if (firstFrame < 0) firstFrame = paper()->height(); });
    show("m3", "other@x.test");
    check(body()->property("result").toMap()["id"] == "m3", "a held message is shown without waiting for the engine");
    check(await([&] { return firstFrame >= 0; }), "the message is painted again");
    QObject::disconnect(frames);
    check(settled > 0 && firstFrame == settled, "a message shown again has its final height on its first frame");
    check(service.requests.size() == requestsBefore, "its loaded images are reused");
    check(await(ready) && paper()->height() == settled, "the height stays once it lays out");

    // Asked for, the quoted history loads as its own view, with its own images.
    check(body()->property("result").toMap()["quote_removed"] == true, "the reader's view leaves the history out");
    body()->setProperty("quotes", true);
    check(await([&] { return ready() && view()->property("text").toString().contains("Quoted answer"); }), "the quoted history shows when asked for");
    check(await([&] { return service.requests.contains("m3:2q"); }), "images are asked for in the view that shows them");
    body()->setProperty("addedPhrases", QStringList{"Quoted answer"});
    check(await([&] { return view()->property("highlightCount").toInt() == 1; }), "a line the original does not have is tinted");
    body()->setProperty("matchPhrases", QStringList{"news", "ad"});
    check(await([&] { return view()->property("highlightCount").toInt() == 2; }), "a search's matches take the tint's place while shown");
    body()->setProperty("matchPhrases", QStringList{});
    body()->setProperty("addedPhrases", QStringList{});
    body()->setProperty("quotes", false);
    check(await([&] { return ready() && !view()->property("text").toString().contains("Quoted answer"); }), "hiding it again shows the reader's view");

    // Plain-only results ask the reader for Text.
    service.plainOnly = true;
    show("m4", "x@x.test");
    check(await([&] { return body() && body()->property("failed").toBool(); }), "a message without HTML falls back to Text");

    // Proxy settings save the host and send the password to the vault call only.
    auto *settings = find(window->contentItem(), "settings");
    find(settings, "remoteProxyHost")->setProperty("text", "proxy.example.net:8443");
    find(settings, "remoteProxyUsername")->setProperty("text", "user");
    find(settings, "remoteProxyPassword")->setProperty("text", "secret");
    settings->setProperty("editing", true);
    QMetaObject::invokeMethod(settings, "save", Q_ARG(QVariant, true));
    check(service.savedProxy["host"] == "proxy.example.net:8443" && service.savedProxy["enabled"] == true && service.password == "secret", "saving sends settings and the password separately");
    check(find(settings, "remoteProxyPassword")->property("text").toString().isEmpty(), "the password field clears after saving");
    QMetaObject::invokeMethod(find(settings, "remoteProxyTest"), "clicked");
    check(service.tests == 1 && await([&] { return find(settings, "remoteProxyStatus")->property("text").toString().contains("42 ms"); }), "Test reports the proxy round trip");
    find(settings, "remoteProxyHost")->setProperty("text", "https://proxy.example.net");
    QMetaObject::invokeMethod(settings, "save", Q_ARG(QVariant, true));
    check(find(settings, "remoteProxyStatus")->property("text").toString().contains("without https://"), "an invalid host explains itself");
    check(find(settings, "remoteImageCacheSize")->property("text").toString().startsWith("1.0 MB of 256.0 MB"), "the cache size is shown");

    check(!warnings, "no QML warnings");
    std::fprintf(stderr, "%d failure(s)\n", failures);
    return failures ? 1 : 0;
}

#include "HtmlMessageBodyTest.moc"
