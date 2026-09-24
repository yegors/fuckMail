#include "ConversationListModel.h"
#include <QSet>

ConversationListModel::ConversationListModel(QObject *parent) : QAbstractListModel(parent) {}

int ConversationListModel::rowCount(const QModelIndex &parent) const {
    return parent.isValid() ? 0 : int(m_rows.size());
}

QVariant ConversationListModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.model() != this || index.column() != 0
        || index.row() < 0 || index.row() >= m_rows.size()) return {};
    if (role == RowDataRole) return m_rows[index.row()].value;
    if (role == PreviousGroupRole) return m_rows[index.row()].previousGroup;
    return {};
}

QHash<int, QByteArray> ConversationListModel::roleNames() const {
    return {{RowDataRole, "rowData"}, {PreviousGroupRole, "previousGroup"}};
}

QString ConversationListModel::group(const Row &row) {
    // Inbox rows carry their section, which splits Done by month.
    const auto section = row.value.value("section").toString();
    if (!section.isEmpty()) return section;
    const auto display = row.value.value("display_group").toString();
    if (!display.isEmpty()) return display;
    const auto value = row.value.value("group").toString();
    return value.isEmpty() ? QStringLiteral("done") : value;
}

bool ConversationListModel::replace(const QString &scope, const QVariantList &rows) {
    QVector<Row> next;
    next.reserve(rows.size());
    QSet<Key> keys;
    for (const auto &value : rows) {
        if (value.metaType().id() != QMetaType::QVariantMap) return false;
        const auto map = value.toMap();
        const auto id = map.value("id"), account = map.value("account");
        if (id.metaType().id() != QMetaType::QString || id.toString().isEmpty()
            || (account.isValid() && account.metaType().id() != QMetaType::QString)) return false;
        const Key key{account.toString(), id.toString()};
        if (keys.contains(key)) return false;
        keys.insert(key);
        next.append({key, map, next.isEmpty() ? QString{} : group(next.last())});
    }
    if (scope != m_scope) {
        beginResetModel();
        m_scope = scope;
        m_rows = std::move(next);
        endResetModel();
        return true;
    }
    if (next == m_rows) return true;
    bool structural = false;

    for (int last = int(m_rows.size()) - 1; last >= 0;) {
        if (keys.contains(m_rows[last].key)) { --last; continue; }
        int first = last;
        while (first > 0 && !keys.contains(m_rows[first - 1].key)) --first;
        beginRemoveRows({}, first, last);
        structural = true;
        m_rows.remove(first, last - first + 1);
        endRemoveRows();
        last = first - 1;
    }
    QHash<Key, int> positions;
    for (int i = 0; i < m_rows.size(); ++i) positions.insert(m_rows[i].key, i);
    // Fix one prefix position at a time. Existing rows move upwards into the
    // prefix: Qt's destinationChild is the pre-removal insertion point i.
    // Equal/data-only/append refreshes are O(N). A full reorder is O(N^2)
    // from vector moves/index maintenance, bounded by loaded rows, not mail size.
    for (int i = 0; i < next.size(); ++i) {
        if (i < m_rows.size() && m_rows[i].key == next[i].key) continue;
        const auto found = positions.constFind(next[i].key);
        if (found == positions.cend()) {
            int last = i;
            while (last + 1 < next.size() && !positions.contains(next[last + 1].key)) ++last;
            beginInsertRows({}, i, last);
            structural = true;
            for (int added = i; added <= last; ++added) m_rows.insert(added, next[added]);
            endInsertRows();
            for (int changed = i; changed < m_rows.size(); ++changed)
                positions.insert(m_rows[changed].key, changed);
            i = last;
        } else {
            const int source = found.value();
            Q_ASSERT(source > i);
            const bool moving = beginMoveRows({}, source, source, {}, i);
            Q_ASSERT(moving);
            if (!moving) qFatal("Conversation row move violated the validated prefix invariant");
            structural = true;
            m_rows.move(source, i);
            endMoveRows();
            for (int changed = i; changed <= source; ++changed)
                positions.insert(m_rows[changed].key, changed);
        }
    }
    // Keep neighbor presentation as stored role data through structural events.
    // Inserted rows already carry their final predecessor; moved/surviving rows
    // retain their old value until this explicit notification changes it.
    for (int i = 0; i < next.size(); ++i) {
        QList<int> changed;
        if (m_rows[i].value != next[i].value) {
            m_rows[i].value = next[i].value;
            changed.append(RowDataRole);
        }
        if (m_rows[i].previousGroup != next[i].previousGroup) {
            m_rows[i].previousGroup = next[i].previousGroup;
            changed.append(PreviousGroupRole);
        }
        if (!structural && !changed.isEmpty()) emit dataChanged(index(i, 0), index(i, 0), changed);
    }
    // A Quick view can cache roles while consuming successive structural
    // changes. Publish the final roles once, after all indexes have settled,
    // without resetting surviving delegates. Data-only updates stay narrow.
    if (structural && !m_rows.isEmpty()) emit dataChanged(index(0, 0), index(m_rows.size() - 1, 0), {RowDataRole, PreviousGroupRole});
    return true;
}
