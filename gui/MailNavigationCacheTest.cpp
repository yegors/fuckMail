#include "MailBackend.h"
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
#include <cstdlib>

namespace {
void require(bool ok, const char *message) { if (!ok) { qCritical() << message; std::exit(1); } }
QVariantMap account(const QString &id, int revision = 1, const QString &state = "active") {
    return {{"id",id},{"lifecycle",QVariantMap{{"state",state},{"revision",revision}}}};
}
QVariantMap reader(const QString &body, int revision) {
    return {{"revision",revision},{"messages",QVariantList{QVariantMap{{"id","message"},{"body",body},{"unread",true}}}}};
}
}

class MailNavigationCacheTest {
    static void visibleRefresh() {
        using GoogleOperationFixture::await;
        MailBackend backend("unused", "unused");
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
            if (name != "Theme.qml") module += name.chopped(4).toUtf8() + " 1.0 " + name.toUtf8() + '\n';
        }
        QFile registration(modulePath + "/qmldir"); require(registration.open(QIODevice::WriteOnly), "QML registration");
        registration.write(module); registration.close();
        qmlRegisterType<ReplyDocument>("NavigationTest", 1, 0, "ReplyDocument");
        for (const auto &weight : {"Regular", "Medium", "SemiBold"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf") >= 0, "actual bundled font");
        QGuiApplication::setFont(QFont("Geist", 10));
        QQmlApplicationEngine qml; bool warnings = false;
        QObject::connect(&qml, &QQmlEngine::warnings, [&](const auto &errors) { warnings = true; for (const auto &error : errors) qWarning() << error.toString(); });
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport NavigationTest 1.0\nApplicationWindow { visible: true; width: 1440; height: 700; color: Theme.canvas; palette.text: Theme.text; palette.base: Theme.surface; palette.window: Theme.canvas; palette.button: Theme.surface; palette.buttonText: Theme.text; property bool singleKeysAllowed: true; property bool searchPopupOpen: false; Binding { target: Theme; property: 'themeId'; value: backend.preferences.theme } Binding { target: Theme; property: 'textSize'; value: backend.preferences.text_size } MailView { anchors.fill: parent } }", QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!qml.rootObjects().isEmpty(), "actual MailView loads");
        auto *window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); require(window, "native MailView window");
        await([&] { return window->isExposed(); }, "MailView is exposed");
        const auto frame = [&] {
            bool painted = false; const auto connection = QObject::connect(window, &QQuickWindow::frameSwapped, [&] { painted = true; });
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
        require(!warnings, "structural refresh has no QML errors");
        qInfo() << "Actual MailView retained unique visible rows across 100-to-201 structural refresh.";
    }
public:
    static void run() {
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
