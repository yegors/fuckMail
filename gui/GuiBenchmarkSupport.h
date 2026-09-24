#pragma once
#include <QJsonArray>
#include <QJsonObject>
#include <QQuickItem>
#include <QQuickWindow>
#include <QtMath>
#include <algorithm>

// Shared developer measurement helpers for the native benchmarks.
namespace GuiBenchmarkSupport {
// A named item whose whole ancestor chain is visible.
inline QQuickItem *findItem(QQuickItem *parent, const QString &name) {
    if (!parent->isVisible()) return nullptr;
    if (parent->objectName() == name) return parent;
    for (auto child : parent->childItems()) if (auto found = findItem(child, name)) return found;
    return nullptr;
}
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
// Every visible item below parent whose objectName starts with prefix.
inline void namedItems(QQuickItem *parent, const QString &prefix, QList<QQuickItem *> &result) {
    if (!parent->isVisible()) return;
    if (parent->objectName().startsWith(prefix)) result.append(parent);
    for (auto child : parent->childItems()) namedItems(child, prefix, result);
}
inline QJsonObject summary(const QJsonArray &samples, const QString &field) {
    QList<double> values;
    for (const auto &sample : samples) values.append(sample.toObject()[field].toDouble());
    if (values.isEmpty()) return {};
    std::sort(values.begin(), values.end());
    auto percentile = [&values](double p) { return values[qCeil((values.size() - 1) * p)]; };
    return {{"p50_ms", percentile(.5)}, {"p95_ms", percentile(.95)}, {"p99_ms", percentile(.99)}, {"max_ms", values.last()}};
}
}
