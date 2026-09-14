#pragma once

#include <QAbstractListModel>
#include <QPair>
#include <QVariantList>
#include <QVariantMap>
#include <QVector>

// Presentation adapter only. MailBackend retains the canonical QVariantList.
class ConversationListModel final : public QAbstractListModel {
    Q_OBJECT
public:
    enum Role { RowDataRole = Qt::UserRole + 1, PreviousGroupRole };
    explicit ConversationListModel(QObject *parent = nullptr);
    int rowCount(const QModelIndex &parent = {}) const override;
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    // Scope includes account and view identity. False means invalid input;
    // nothing changed. True includes a valid no-op. Never resets within scope.
    bool replace(const QString &scope, const QVariantList &rows);

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
