#include "ConversationListModel.h"
#include <QAbstractItemModelTester>
#include <QCoreApplication>
#include <QPersistentModelIndex>
#include <QSignalSpy>
#include <QtTest>

namespace {
QVariantMap row(const QString &id, const QString &group = "review", const QString &account = "personal") {
    return {{"id", id}, {"account", account}, {"group", group}, {"subject", "Subject " + id}, {"unread", true}};
}
QString idAt(const ConversationListModel &model, int index) {
    return model.index(index, 0).data(ConversationListModel::RowDataRole).toMap().value("id").toString();
}
QString previousAt(const ConversationListModel &model, int index) {
    return model.index(index, 0).data(ConversationListModel::PreviousGroupRole).toString();
}
QString displayedGroup(const QVariant &value) {
    const auto map = value.toMap();
    const auto display = map.value("display_group").toString();
    if (!display.isEmpty()) return display;
    const auto group = map.value("group").toString();
    return group.isEmpty() ? QStringLiteral("done") : group;
}
void verifyRows(const ConversationListModel &model, const QVariantList &expected) {
    QCOMPARE(model.rowCount(), expected.size());
    for (int index = 0; index < expected.size(); ++index) {
        QCOMPARE(model.index(index, 0).data(ConversationListModel::RowDataRole).toMap(), expected[index].toMap());
        QCOMPARE(previousAt(model, index), index ? displayedGroup(expected[index - 1]) : QString{});
    }
}
}

class ConversationListModelTest final : public QObject {
    Q_OBJECT
private slots:
    void dataOnlyAndNoopPreserveIdentity() {
        ConversationListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::Fatal);
        QVariantList rows{row("a"), row("b")};
        QVERIFY(model.replace("personal/inbox", rows));
        const QPersistentModelIndex first(model.index(0, 0)), second(model.index(1, 0));
        QSignalSpy changed(&model, &QAbstractItemModel::dataChanged);
        QSignalSpy reset(&model, &QAbstractItemModel::modelReset);
        QSignalSpy inserted(&model, &QAbstractItemModel::rowsInserted);
        QSignalSpy removed(&model, &QAbstractItemModel::rowsRemoved);
        QSignalSpy moved(&model, &QAbstractItemModel::rowsMoved);
        QVERIFY(model.replace("personal/inbox", rows));
        QCOMPARE(changed.count(), 0);
        auto firstRow = rows[0].toMap();
        firstRow["unread"] = false; firstRow["group"] = "done";
        rows[0] = firstRow;
        QVERIFY(model.replace("personal/inbox", rows));
        verifyRows(model, rows);
        QCOMPARE(first.row(), 0); QCOMPARE(second.row(), 1);
        QCOMPARE(changed.count(), 2);
        QCOMPARE(changed[0][2].value<QList<int>>(), QList<int>{ConversationListModel::RowDataRole});
        QCOMPARE(changed[1][2].value<QList<int>>(), QList<int>{ConversationListModel::PreviousGroupRole});
        QCOMPARE(reset.count() + inserted.count() + removed.count() + moved.count(), 0);
        changed.clear();
        QVERIFY(model.replace("personal/inbox", rows));
        QCOMPARE(changed.count(), 0);
    }

    void structuralUpdatesPreserveSurvivors() {
        ConversationListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::Fatal);
        QVERIFY(model.replace("personal/inbox", {row("a"), row("b"), row("c"), row("d"), row("e")}));
        const QPersistentModelIndex a(model.index(0, 0)), b(model.index(1, 0)), c(model.index(2, 0)), removedIndex(model.index(3, 0));
        QSignalSpy reset(&model, &QAbstractItemModel::modelReset);
        QSignalSpy inserted(&model, &QAbstractItemModel::rowsInserted);
        QSignalSpy removed(&model, &QAbstractItemModel::rowsRemoved);
        QSignalSpy moved(&model, &QAbstractItemModel::rowsMoved);
        const QVariantList next{row("x"), row("c", "held"), row("a"), row("y", "done"), row("b")};
        QVERIFY(model.replace("personal/inbox", next));
        verifyRows(model, next);
        QCOMPARE(a.row(), 2); QCOMPARE(b.row(), 4); QCOMPARE(c.row(), 1);
        QCOMPARE(a.data(ConversationListModel::RowDataRole).toMap().value("id").toString(), "a");
        QVERIFY(!removedIndex.isValid());
        QCOMPARE(reset.count(), 0); QCOMPARE(inserted.count(), 2); QCOMPARE(removed.count(), 1);
        QVERIFY(moved.count() > 0);
        const QVariantList reversed{next[4], next[3], next[2], next[1], next[0]};
        QVERIFY(model.replace("personal/inbox", reversed));
        verifyRows(model, reversed);
        QCOMPARE(a.row(), 2); QCOMPARE(b.row(), 0); QCOMPARE(c.row(), 3);
        QCOMPARE(reset.count(), 0);
    }

    void predecessorRoleChangesAreObservable() {
        ConversationListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::Fatal);
        QVERIFY(model.replace("personal/inbox", {row("a"), row("c", "held")}));
        QHash<QString, QString> observed{{"a", previousAt(model, 0)}, {"c", previousAt(model, 1)}};
        // A consumer caches roles when an item is inserted, then updates only
        // roles named by dataChanged. A derived but unnotified neighbor fails.
        connect(&model, &QAbstractItemModel::rowsInserted, this, [&](const QModelIndex &, int first, int last) {
            for (int index = first; index <= last; ++index) observed[idAt(model, index)] = previousAt(model, index);
        });
        connect(&model, &QAbstractItemModel::dataChanged, this, [&](const QModelIndex &first, const QModelIndex &last, const QList<int> &roles) {
            if (!roles.isEmpty() && !roles.contains(ConversationListModel::PreviousGroupRole)) return;
            for (int index = first.row(); index <= last.row(); ++index) observed[idAt(model, index)] = previousAt(model, index);
        });
        auto a = row("a", "done");
        QVariantList next{a, row("b"), row("c", "held")};
        QVERIFY(model.replace("personal/inbox", next));
        for (int index = 0; index < model.rowCount(); ++index) QCOMPARE(observed[idAt(model, index)], previousAt(model, index));
        QCOMPARE(observed.value("b"), "done");
        a["display_group"] = "review"; a["pinned"] = true;
        next = {row("c", "done"), a, row("b", "held")};
        QVERIFY(model.replace("personal/inbox", next));
        verifyRows(model, next);
        for (int index = 0; index < model.rowCount(); ++index) QCOMPARE(observed[idAt(model, index)], previousAt(model, index));
        QCOMPARE(observed.value("b"), "review");
    }

    void pageAppendAndScopeReset() {
        ConversationListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::Fatal);
        QVariantList rows;
        for (int index = 0; index < 50; ++index) rows.append(row(QString::number(index)));
        QVERIFY(model.replace("personal/inbox", rows));
        QPersistentModelIndex retained(model.index(20, 0));
        QSignalSpy inserted(&model, &QAbstractItemModel::rowsInserted), reset(&model, &QAbstractItemModel::modelReset);
        for (int index = 50; index < 100; ++index) rows.append(row(QString::number(index), "done"));
        QVERIFY(model.replace("personal/inbox", rows));
        verifyRows(model, rows);
        QCOMPARE(inserted.count(), 1); QCOMPARE(inserted[0][1].toInt(), 50); QCOMPARE(inserted[0][2].toInt(), 99);
        QCOMPARE(retained.row(), 20); QCOMPARE(reset.count(), 0);
        QVERIFY(model.replace("workspace/inbox", {row("20", "review", "workspace")}));
        QVERIFY(!retained.isValid()); QCOMPARE(reset.count(), 1);
        QCOMPARE(model.index(0, 0).data(ConversationListModel::RowDataRole).toMap().value("account").toString(), "workspace");
        retained = model.index(0, 0);
        QVERIFY(model.replace("workspace/search", {row("20", "review", "workspace")}));
        QVERIFY(!retained.isValid()); QCOMPARE(reset.count(), 2);
        QVERIFY(model.replace("workspace/search", {}));
        QCOMPARE(model.rowCount(), 0); QCOMPARE(reset.count(), 2);
    }

    void accountInventoryUpdatesKeepTabIdentity() {
        ConversationListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::Fatal);
        // Account inventory entries have their own id, without an account or
        // conversation group field. Background progress must not reset tabs.
        QVariantList accounts{QVariantMap{{"id", "personal"}, {"name", "Personal"}, {"unread_conversation_count", 30}, {"held_conversation_count", 0}},
            QVariantMap{{"id", "workspace"}, {"name", "Workspace"}, {"unread_conversation_count", 20}, {"held_conversation_count", 0}}};
        QVERIFY(model.replace("account-inventory", accounts));
        const QPersistentModelIndex personal(model.index(0, 0)), workspace(model.index(1, 0));
        QSignalSpy changed(&model, &QAbstractItemModel::dataChanged), reset(&model, &QAbstractItemModel::modelReset);
        QSignalSpy inserted(&model, &QAbstractItemModel::rowsInserted), removed(&model, &QAbstractItemModel::rowsRemoved);
        auto next = accounts[1].toMap(); next["held_conversation_count"] = 1;
        next["coverage"] = QVariantMap{{"messages", 200}}; accounts[1] = next;
        QVERIFY(model.replace("account-inventory", accounts));
        verifyRows(model, accounts);
        QCOMPARE(changed.count(), 1); QCOMPARE(changed[0][2].value<QList<int>>(), QList<int>{ConversationListModel::RowDataRole});
        QCOMPARE(personal.row(), 0); QCOMPARE(workspace.row(), 1);
        QCOMPARE(reset.count() + inserted.count() + removed.count(), 0);
        QVERIFY(model.replace("account-inventory", {accounts[1]}));
        QVERIFY(!personal.isValid()); QVERIFY(workspace.isValid()); QCOMPARE(workspace.row(), 0);
        QCOMPARE(reset.count(), 0);
    }

    void invalidIdentityDoesNotPartiallyPublish() {
        ConversationListModel model;
        QAbstractItemModelTester tester(&model, QAbstractItemModelTester::FailureReportingMode::Fatal);
        const QVariantList original{row("a")};
        QVERIFY(model.replace("personal/inbox", original));
        const QPersistentModelIndex retained(model.index(0, 0));
        QSignalSpy changed(&model, &QAbstractItemModel::dataChanged), reset(&model, &QAbstractItemModel::modelReset);
        QSignalSpy inserted(&model, &QAbstractItemModel::rowsInserted), removed(&model, &QAbstractItemModel::rowsRemoved);
        QVERIFY(!model.replace("different-scope", {row("b"), row("b")}));
        QVERIFY(!model.replace("personal/inbox", {row("")}));
        QVERIFY(!model.replace("personal/inbox", {QStringLiteral("not a row")}));
        auto invalid = row("b"); invalid["account"] = 42;
        QVERIFY(!model.replace("personal/inbox", {invalid}));
        verifyRows(model, original); QVERIFY(retained.isValid());
        QCOMPARE(changed.count() + reset.count() + inserted.count() + removed.count(), 0);
        // The backend's rejection path clears both canonical and native rows.
        QVERIFY(model.replace({}, {}));
        QCOMPARE(model.rowCount(), 0); QVERIFY(!retained.isValid());
    }
};

QTEST_GUILESS_MAIN(ConversationListModelTest)
#include "ConversationListModelTest.moc"
