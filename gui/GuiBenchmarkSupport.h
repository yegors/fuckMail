#pragma once
#include <QJsonArray>
#include <QJsonObject>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtMath>
#include <algorithm>

/// Shared developer measurement helpers for the native benchmarks.
/// GUI thread only, like the item trees they walk.
namespace GuiBenchmarkSupport {
/// A named item whose whole ancestor chain is visible.
/// Depth-first below and including `parent`; null when none is found.
inline QQuickItem *findItem(QQuickItem *parent, const QString &name) {
    if (!parent->isVisible()) return nullptr;
    if (parent->objectName() == name) return parent;
    for (auto child : parent->childItems()) if (auto found = findItem(child, name)) return found;
    return nullptr;
}
/// The part of `item` a user could see, in scene (window logical pixel)
/// coordinates: clipped to the window and to every clipping ancestor. Empty
/// when the item or an ancestor is hidden, fully transparent or zero-sized.
inline QRectF visibleRect(QQuickItem *item, QQuickWindow *window) {
    if (!item || !item->isVisible() || item->opacity() <= 0 || item->width() <= 0 || item->height() <= 0) return {};
    auto rect = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
    rect = rect.intersected(QRectF(0, 0, window->width(), window->height()));
    for (auto parent = item->parentItem(); parent; parent = parent->parentItem()) {
        if (!parent->isVisible() || parent->opacity() <= 0) return {};
        if (parent->clip()) rect = rect.intersected(parent->mapRectToScene(QRectF(0, 0, parent->width(), parent->height())));
    }
    return rect;
}
/// Every visible item below parent whose objectName starts with prefix.
/// Appends to `result` in depth-first order; hidden subtrees are skipped.
inline void namedItems(QQuickItem *parent, const QString &prefix, QList<QQuickItem *> &result) {
    if (!parent->isVisible()) return;
    if (parent->objectName().startsWith(prefix)) result.append(parent);
    for (auto child : parent->childItems()) namedItems(child, prefix, result);
}
/// p50, p95, p99 and maximum of `field` across `samples`, whose values are
/// milliseconds. A percentile p is the sorted value at index ceil((n - 1) * p),
/// with no interpolation. Empty when there are no samples.
inline QJsonObject summary(const QJsonArray &samples, const QString &field) {
    QList<double> values;
    for (const auto &sample : samples) values.append(sample.toObject()[field].toDouble());
    if (values.isEmpty()) return {};
    std::sort(values.begin(), values.end());
    auto percentile = [&values](double p) { return values[qCeil((values.size() - 1) * p)]; };
    return {{"p50_ms", percentile(.5)}, {"p95_ms", percentile(.95)}, {"p99_ms", percentile(.99)}, {"max_ms", values.last()}};
}
}
