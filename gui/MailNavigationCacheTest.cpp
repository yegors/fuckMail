#include "MailBackend.h"
#include <QGuiApplication>
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
    }
};
int main(int argc, char **argv) { QGuiApplication app(argc,argv); MailNavigationCacheTest::run(); return 0; }
