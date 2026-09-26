#pragma once
#include <QColor>
#include <QImage>
#include <QQuickItem>

class FmIconCache;

/// Line icons for FmIcon.qml, drawn with the same 16×16 geometry the former
/// Canvas used. Each window draws a name, colour and pixel size once and shares
/// that texture, so an icon costs one item instead of a canvas or an image.
///
/// Properties are set on the GUI thread; updatePaintNode() runs on the render
/// thread while the GUI thread is blocked, as usual for QQuickItem. Textures
/// belong to a per-window cache that is parented to the QQuickWindow and
/// dropped when its scene graph is invalidated.
class FmIconItem : public QQuickItem {
    Q_OBJECT
    /// Icon name from the fixed set drawn in FmIconItem.cpp (for example
    /// "mail", "search", "close"). An unknown or empty name draws nothing.
    Q_PROPERTY(QString name READ name WRITE setName NOTIFY nameChanged)
    /// Stroke colour, or fill colour for the few filled icons.
    Q_PROPERTY(QColor color READ color WRITE setColor NOTIFY colorChanged)
    /// The icon is size×size logical pixels (default 16).
    /// Setting it also sets the item's implicit and actual size, so layout
    /// code should size the icon through this property rather than width/height.
    Q_PROPERTY(int size READ iconSize WRITE setIconSize NOTIFY sizeChanged)
public:
    explicit FmIconItem(QQuickItem *parent = nullptr);
    QString name() const { return m_name; }
    void setName(const QString &name);
    QColor color() const { return m_color; }
    void setColor(const QColor &color);
    int iconSize() const { return m_size; }
    void setIconSize(int size);
    /// Draws icon `name` into a transparent premultiplied image of `pixels`
    /// device pixels. `scale` maps the 16×16 grid onto the image in device
    /// pixels. Strokes are 1.5 grid units wide. The texture cache calls it on
    /// the render thread; it reads no item or window state.
    static QImage render(const QString &name, const QColor &color, const QSize &pixels, const QSizeF &scale);
    /// Registers FmIconItem in FuckMail.Icons, and FmHtmlView in FuckMail.Html;
    /// call before loading FmIcon.qml. Safe to call more than once.
    static void registerType();
signals:
    void nameChanged();
    void colorChanged();
    void sizeChanged();
protected:
    QSGNode *updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData &data) override;
private:
    QString m_name;
    QColor m_color = Qt::black;
    int m_size = 16;
    FmIconCache *m_cache = nullptr;
};
