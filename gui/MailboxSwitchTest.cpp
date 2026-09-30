// MailView UI test for switching mailboxes, on a real MailBackend over the
// scripted engine: an account seen or warmed this session shows its rows at
// once, and one still loading never claims to be empty or clear.
#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontDatabase>
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
QVariantMap account(const QString &id) {
    return {{"id", id}, {"name", id}, {"email", id + "@example.test"}, {"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}},
        {"inbox_review_count", 3}, {"google", QVariantMap{{"capabilities", QVariantMap{{"mail_read", true}, {"mail_write", true}}}}}};
}
QJsonArray rows(const QString &account) {
    QJsonArray list;
    for (int i = 0; i < 3; ++i)
        list.append(QJsonObject{{"id", account + "-" + QString::number(i)}, {"account", account}, {"subject", "Mail " + QString::number(i)},
            {"sender_name", "Sender"}, {"sender_email", "sender@example.test"}, {"latest_at", "2026-09-28T09:00:00Z"}, {"group", "review"},
            {"unread", true}, {"starred", false}, {"message_count", 1}, {"revision", 1}});
    return list;
}
struct Held { QPointer<QLocalSocket> peer; QString id, method, account; };
}

class MailboxSwitchTest {
public:
static void run() {
    for (const auto weight : {"Regular", "Medium", "SemiBold"})
        require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf") >= 0, "bundled font");
    Engine fixture; MailBackend backend(fixture.profile, "unused-fixture-engine", nullptr, false, true);
    backend.m_connected = true; backend.m_syntheticProfile = false;
    backend.m_account = "first"; backend.m_folder = "inbox";
    backend.m_enginePid = QCoreApplication::applicationPid();
    backend.m_allAccounts = {account("first"), account("second"), account("third")};
    require(backend.publishAccounts(backend.m_allAccounts), "three accounts");
    // Once set, the third account's replies wait until released, as behind a busy store.
    bool holdThird = false; QList<Held> held;
    const auto answer = [&](QLocalSocket *peer, const QString &id, const QString &method, const QString &account) {
        if (method == "view_get") peer->write(reply(id, {{"revision", 1}, {"view", QJsonObject{{"folder", "inbox"}}}}));
        else if (method == "conversations") peer->write(reply(id, {{"conversations", rows(account)}, {"filtered_conversation_count", 3}, {"next_offset", QJsonValue::Null}}));
        else if (method == "conversation") peer->write(reply(id, {{"id", account + "-0"}, {"account", account}, {"revision", 1}, {"subject", "Mail 0"}, {"message_ids", QJsonArray{"m"}},
            {"messages", QJsonArray{QJsonObject{{"id", account + "-m"}, {"account", account}, {"sender_name", "Sender"}, {"sender_email", "sender@example.test"}, {"text", "Hello"}, {"sent", false}, {"unread", false}, {"sent_at", "2026-09-28T09:00:00Z"}}}}}));
        else if (method == "accounts") peer->write(reply(id, {{"accounts", QJsonArray::fromVariantList(backend.m_allAccounts)}}));
        else if (method == "assist_settings_get") peer->write(reply(id, {{"revision", 0}, {"settings", QJsonObject{{"accounts", QJsonObject{}}, {"model", QJsonValue::Null}}}}));
        else if (method == "view_save") peer->write(reply(id, {{"revision", 2}}));
        else peer->write(reply(id, {}));
    };
    fixture.customRequest = [&](QLocalSocket *peer, const QJsonObject &request) {
        const auto method = request["method"].toString(), id = request["id"].toString(), account = request["account"].toString();
        if (holdThird && account == "third" && (method == "view_get" || method == "conversations")) { held.append({peer, id, method, account}); return true; }
        answer(peer, id, method, account);
        return true;
    };
    QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
    backend.m_socket.connectToServer(fixture.server.serverName());
    await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached socket");
    bool hello = false; backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
    await([&] { return hello; }, "cached hello");

    QTemporaryDir copy; const auto modulePath = copy.filePath("SwitchTest"); require(QDir().mkpath(modulePath), "module directory");
    QByteArray registration("module SwitchTest\nsingleton Theme 1.0 Theme.qml\n");
    for (const auto &name : QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"}, QDir::Files)) {
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + name, modulePath + '/' + name), "production QML copy");
        if (name == "ShortcutRegistry.qml") registration += "singleton ";
        if (name != "Theme.qml") registration += name.chopped(4).toUtf8() + " 1.0 " + name.toUtf8() + '\n';
    }
    QFile module(modulePath + "/qmldir"); require(module.open(QIODevice::WriteOnly), "module registration");
    module.write(registration); module.close();
    qmlRegisterType<ReplyDocument>("SwitchTest", 1, 0, "ReplyDocument");
    QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings = false;
    QObject::connect(&qml, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { warnings = true; for (const auto &error : errors) qWarning() << error; });
    qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
    qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import SwitchTest 1.0
ApplicationWindow {width:1280;height:800;visible:true;color:Theme.bg
 property bool singleKeysAllowed:true;property bool searchPopupOpen:false
 MailView {anchors.fill:parent}
})QML", QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(), "production mail workspace loads");
    auto window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); require(window, "window");
    await([&] { return window->isExposed(); }, "workspace exposed");
    auto item = [&](const QString &name) { return find(window->contentItem(), name); };
    const auto shown = [&](const QString &name) { auto found = item(name); return found && found->isVisible(); };
    // What the reader says while nothing is open: never "clear" or "nothing"
    // for an account whose list is still on its way.
    const auto claimsEmpty = [&] {
        auto title = item("emptyReaderTitle");
        return shown("inboxClearTitle") || (title && title->isVisible() && title->property("text").toString().startsWith("Nothing"));
    };

    backend.loadConversations();
    await([&] { return backend.m_conversations.size() == 3; }, "the first account's list");
    // A moment later the other accounts are read ahead.
    await([&] { return backend.m_mailViewCache.contains("second") && backend.m_mailViewCache.contains("third"); }, "the other accounts are warmed");

    // Switching to a warmed account shows its rows in the same frame.
    backend.selectAccount("second");
    require(backend.m_conversations.size() == 3 && backend.m_conversations.first().toMap().value("account") == "second", "warmed rows show at once");
    QCoreApplication::processEvents();
    require(!claimsEmpty(), "a warmed account never claims to be empty");

    // Switching to one with nothing in memory waits without claiming anything.
    backend.m_mailViewCache.remove("third"); holdThird = true;
    backend.selectAccount("third");
    require(backend.m_conversations.isEmpty() && backend.loading(), "the list is loading, not empty");
    QElapsedTimer waited; waited.start();
    while (waited.elapsed() < 600) {
        QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
        if (claimsEmpty()) require(false, "a loading account never shows Inbox clear or Nothing open");
    }
    holdThird = false;
    for (const auto &request : held) if (request.peer) answer(request.peer, request.id, request.method, request.account);
    await([&] { return backend.m_conversations.size() == 3 && !backend.loading(); }, "the held list arrives");
    require(backend.m_conversations.first().toMap().value("account") == "third", "rows belong to the account switched to");

    // Back to the first account: seen this session, so at once again.
    backend.selectAccount("first");
    require(backend.m_conversations.size() == 3 && backend.m_conversations.first().toMap().value("account") == "first", "a visited account shows at once");
    require(!warnings, "production QML has no warnings");
    qInfo() << "PASS: warmed and visited mailboxes switch at once; a loading one never claims to be empty";
}
};
int main(int argc, char **argv) { QGuiApplication app(argc, argv); QGuiApplication::setFont(QFont("Geist", 10)); QQuickStyle::setStyle("Basic"); MailboxSwitchTest::run(); return 0; }
