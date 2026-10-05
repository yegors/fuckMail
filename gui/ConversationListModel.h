// Qt list model exposing MailBackend's row lists (accounts, conversations, a
// thread's messages) to QML; rows come from engine query results and are
// updated in place rather than reset.
#pragma once

#include <QAbstractListModel>
#include <QPair>
#include <QVariantList>
#include <QVariantMap>
#include <QVector>

/// Presentation adapter only. MailBackend retains the canonical QVariantList.
///
/// MailBackend owns three instances as members (accounts, the conversation
/// list and the open thread's messages) and exposes them to QML as
/// QAbstractItemModel properties. GUI thread only.
///
/// Rows are identified by the pair (`account`, `id`). Keeping that identity
/// stable across refreshes lets ListView keep delegates, focus and scroll
/// position instead of rebuilding the list.
class ConversationListModel final : public QAbstractListModel {
    Q_OBJECT
public:
    /// `rowData` is the row's whole QVariantMap. `previousGroup` is the group
    /// of the row above it ("" for the first row), which QML compares with
    /// the row's own group to decide whether to draw a section header.
    /// `itemType` is the row's `item_type`: "header" or "empty" for the
    /// Inbox's section items, otherwise "row".
    enum Role { RowDataRole = Qt::UserRole + 1, PreviousGroupRole, ItemTypeRole };
    explicit ConversationListModel(QObject *parent = nullptr);
    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    /// Replaces the rows with `rows`, a list of QVariantMaps.
    ///
    /// Each row needs a non-empty string `id`; `account`, when present, must
    /// also be a string; (`account`, `id`) must be unique. A different
    /// `scope` resets the model. The same scope applies the difference as
    /// row removals, inserts and moves, followed by dataChanged for changed
    /// roles.
    ///
    /// Scope includes account and view identity. False means invalid input;
    /// nothing changed. True includes a valid no-op. Never resets within scope.
    bool replace(const QString &scope, const QVariantList &rows);
    /// The scope of the rows held now.
    const QString &scope() const { return m_scope; }
    /// The index of the row with `id`, or -1.
    int indexOf(const QString &id) const;

private:
    using Key = QPair<QString, QString>;
    struct Row {
        Key key;
        QVariantMap value;
        QString previousGroup;
        bool operator==(const Row &) const = default;
    };
    static QString group(const Row &row);
    QString m_scope;
    QVector<Row> m_rows;
};
