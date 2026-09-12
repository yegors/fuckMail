#include "HatchItem.h"
#include <QQuickWindow>
#include <QSGGeometryNode>
#include <QSGRendererInterface>
#include <QSGVertexColorMaterial>
#include <algorithm>
#include <cmath>
#include <limits>

namespace {
constexpr qreal spacing = 6;
constexpr qreal strokeWidth = 1;
constexpr qreal inverseSqrtTwo = 0.7071067811865475244;
constexpr int verticesPerStripe = 18; // two feather bands plus the stroke core

class HatchNode final : public QSGGeometryNode {
public:
    HatchNode() {
        auto *mesh = new QSGGeometry(QSGGeometry::defaultAttributes_ColoredPoint2D(), 0);
        mesh->setDrawingMode(QSGGeometry::DrawTriangles);
        mesh->setVertexDataPattern(QSGGeometry::DynamicPattern);
        setGeometry(mesh); setFlag(OwnsGeometry);
        setMaterial(new QSGVertexColorMaterial); setFlag(OwnsMaterial);
    }
    int capacity = 0, generated = 0;
    qreal height = -1, dpr = -1;
    QColor color;
};

void stripe(QSGGeometry::ColoredPoint2D *vertices, qreal intercept, qreal height,
    qreal inner, qreal outer, QRgb color) {
    int at = 0;
    const auto point = [&](bool bottom, qreal offset, QRgb rgba) {
        const qreal normal = offset * inverseSqrtTwo;
        vertices[at++].set(float(intercept - (bottom ? height : 0) + normal),
            float((bottom ? height : 0) + normal), uchar(qRed(rgba)), uchar(qGreen(rgba)), uchar(qBlue(rgba)), uchar(qAlpha(rgba)));
    };
    const auto band = [&](qreal from, qreal to, QRgb firstColor, QRgb lastColor) {
        point(true, from, firstColor); point(false, from, firstColor); point(false, to, lastColor);
        point(true, from, firstColor); point(false, to, lastColor); point(true, to, lastColor);
    };
    band(-outer, -inner, 0, color); band(-inner, inner, color, color); band(inner, outer, color, 0);
}
}

HatchItem::HatchItem(QQuickItem *parent) : QQuickItem(parent), m_color(Qt::transparent) {
    setFlag(ItemHasContents); setClip(true);
}

void HatchItem::setColor(const QColor &color) {
    if (m_color == color) return;
    m_color = color; emit colorChanged(); update();
}

void HatchItem::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size()) update();
}

void HatchItem::itemChange(ItemChange change, const ItemChangeData &data) {
    QQuickItem::itemChange(change, data);
    if (change == ItemDevicePixelRatioHasChanged || change == ItemSceneChange) update();
}

QSGNode *HatchItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) {
    // QSGVertexColorMaterial belongs to the accelerated scenegraph. The QML
    // fallback handles software rendering, including packaged offscreen checks.
    if (!window() || window()->rendererInterface()->graphicsApi() == QSGRendererInterface::Software) {
        delete oldNode; return nullptr;
    }
    auto *node = static_cast<HatchNode *>(oldNode);
    if (!node) node = new HatchNode;
    auto *mesh = node->geometry();
    const qreal w = width(), h = height();
    if (!std::isfinite(w) || !std::isfinite(h) || w <= 0 || h <= 0 || m_color.alpha() == 0) {
        if (mesh->vertexCount()) { mesh->setVertexCount(0); node->markDirty(QSGNode::DirtyGeometry); }
        return node;
    }
    const qreal dpr = std::max(qreal(.25), window()->effectiveDevicePixelRatio());
    const qreal feather = 1 / dpr;
    const qreal outer = (strokeWidth + feather) / 2;
    const qreal inner = std::abs(strokeWidth - feather) / 2;
    // Canvas drew (-h+6k,h) -> (6k,0): the same family x+y=6k.
    // Include one extra stripe at each clipped corner for the edge feather.
    const qreal needed = std::ceil((w + h + 2 * outer) / spacing) + 3;
    // The owning overlap item's width/height are bounded by the timeline's
    // viewport. Reject nonsensical geometry before an integer allocation.
    if (needed > std::numeric_limits<int>::max() / verticesPerStripe - 31) {
        mesh->setVertexCount(0); node->markDirty(QSGNode::DirtyGeometry); return node;
    }
    const int stripes = int(needed);
    bool regenerate = node->height != h || node->dpr != dpr || node->color != m_color;
    if (stripes > node->capacity) {
        node->capacity = ((stripes + 31) / 32) * 32;
        mesh->allocate(node->capacity * verticesPerStripe);
        regenerate = true;
    }
    const int first = regenerate ? 0 : std::min(node->generated, stripes);
    if (first < stripes) {
        QColor covered = m_color;
        covered.setAlphaF(covered.alphaF() * std::min(qreal(1), strokeWidth / feather));
        const QRgb premultiplied = qPremultiply(covered.rgba());
        auto *vertices = mesh->vertexDataAsColoredPoint2D();
        for (int i = first; i < stripes; ++i)
            stripe(vertices + i * verticesPerStripe, (i - 1) * spacing, h, inner, outer, premultiplied);
        mesh->markVertexDataDirty();
    }
    if (regenerate) node->generated = stripes;
    else node->generated = std::max(node->generated, stripes);
    const bool countChanged = mesh->vertexCount() != stripes * verticesPerStripe;
    mesh->setVertexCount(stripes * verticesPerStripe);
    if (first < stripes || countChanged) node->markDirty(QSGNode::DirtyGeometry);
    node->height = h; node->dpr = dpr; node->color = m_color;
    return node;
}
