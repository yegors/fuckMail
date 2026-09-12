#pragma once
#include <QColor>
#include <QQuickItem>
#include <QtQml/qqmlregistration.h>

// Native timeline overlap paint: x+y=6k, one logical-pixel stroke, item clipping.
// The software scenegraph keeps the existing Canvas fallback in TodayStrip.qml.
class HatchItem : public QQuickItem {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(QColor color READ color WRITE setColor NOTIFY colorChanged)
public:
    explicit HatchItem(QQuickItem *parent = nullptr);
    QColor color() const { return m_color; }
    void setColor(const QColor &color);
signals:
    void colorChanged();
protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData &data) override;
private:
    QColor m_color;
};
