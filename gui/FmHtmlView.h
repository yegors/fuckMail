#pragma once
#include "HtmlDocument.h"
#include <QColor>
#include <QHash>
#include <QPointF>
#include <QQuickItem>
#include <QSet>
#include <QVariantMap>
#include <memory>

class QSGImageNode;
struct HtmlViewState;

// A sanitised HTML message laid out and painted off the GUI thread. The GUI
// thread holds only a snapshot (size, link boxes, text runs) and texture tiles
// near the viewport. It never loads anything: `images` names local files.
class FmHtmlView : public QQuickItem {
    Q_OBJECT
    // The engine's `message_html` result: html or html_path, plus resources.
    Q_PROPERTY(QVariantMap content READ content WRITE setContent NOTIFY contentChanged)
    // Resource id → {state: hidden|loading|ready|failed|blocked|insecure, path?, width?, height?}.
    Q_PROPERTY(QVariantMap images READ images WRITE setImages NOTIFY imagesChanged)
    Q_PROPERTY(QVariantList blockedLinks READ blockedLinks WRITE setBlockedLinks NOTIFY blockedLinksChanged)
    Q_PROPERTY(qreal fontPixels READ fontPixels WRITE setFontPixels NOTIFY fontPixelsChanged)
    Q_PROPERTY(QColor paper READ paper WRITE setPaper NOTIFY paperChanged)
    // The search's phrases, tinted where they appear (Search 10).
    Q_PROPERTY(QStringList highlightPhrases READ highlightPhrases WRITE setHighlightPhrases NOTIFY highlightChanged)
    Q_PROPERTY(QColor highlightColor READ highlightColor WRITE setHighlightColor NOTIFY highlightChanged)
    Q_PROPERTY(int highlightCount READ highlightCount NOTIFY highlightChanged)
    // "", loading, ready or failed. `failure` says why a message fell back to Text.
    Q_PROPERTY(QString status READ status NOTIFY statusChanged)
    Q_PROPERTY(QString failure READ failure NOTIFY statusChanged)
    Q_PROPERTY(QString text READ text NOTIFY textChanged)
    Q_PROPERTY(qreal contentScale READ contentScale NOTIFY layoutChanged)
    Q_PROPERTY(bool hasSelection READ hasSelection NOTIFY selectionChanged)
    Q_PROPERTY(QString selectedText READ selectedText NOTIFY selectionChanged)
    Q_PROPERTY(int hoveredLink READ hoveredLink NOTIFY hoveredLinkChanged)
    // Item coordinates of the hovered link box.
    Q_PROPERTY(QRectF hoveredLinkRect READ hoveredLinkRect NOTIFY hoveredLinkChanged)
public:
    explicit FmHtmlView(QQuickItem *parent = nullptr);
    ~FmHtmlView() override;
    QVariantMap content() const { return m_content; }
    void setContent(const QVariantMap &content);
    QVariantMap images() const { return m_images; }
    void setImages(const QVariantMap &images);
    QVariantList blockedLinks() const { return m_blockedLinks; }
    void setBlockedLinks(const QVariantList &links);
    qreal fontPixels() const { return m_fontPixels; }
    void setFontPixels(qreal pixels);
    QColor paper() const { return m_paper; }
    void setPaper(const QColor &paper);
    QStringList highlightPhrases() const { return m_highlightPhrases; }
    void setHighlightPhrases(const QStringList &phrases);
    QColor highlightColor() const { return m_highlightColor; }
    void setHighlightColor(const QColor &color);
    int highlightCount() const { return int(m_highlights.size()); }
    QString status() const { return m_status; }
    QString failure() const { return m_failure; }
    QString text() const { return m_snapshot.text; }
    qreal contentScale() const { return m_scale; }
    bool hasSelection() const { return m_selectionEnd > m_selectionStart; }
    QString selectedText() const;
    int hoveredLink() const { return m_hoveredLink; }
    QRectF hoveredLinkRect() const { return m_hoveredRect; }
    Q_INVOKABLE void copySelection() const;
    Q_INVOKABLE void selectAll();
    Q_INVOKABLE void clearSelection();
    // Item-coordinate boxes of one link, for tests and keyboard access.
    Q_INVOKABLE QVariantList linkRects(int id) const;
    Q_INVOKABLE QVariantMap debugState() const;
    static void registerType();
    static qint64 tileBytes();
signals:
    void contentChanged();
    void imagesChanged();
    void blockedLinksChanged();
    void fontPixelsChanged();
    void paperChanged();
    void highlightChanged();
    void statusChanged();
    void textChanged();
    void layoutChanged();
    void selectionChanged();
    void hoveredLinkChanged();
    void linkActivated(int id);
protected:
    QSGNode *updatePaintNode(QSGNode *old, UpdatePaintNodeData *) override;
    void geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) override;
    void itemChange(ItemChange change, const ItemChangeData &data) override;
    void releaseResources() override;
    void updatePolish() override;
    void hoverMoveEvent(QHoverEvent *event) override;
    void hoverLeaveEvent(QHoverEvent *event) override;
    void mousePressEvent(QMouseEvent *event) override;
    void mouseMoveEvent(QMouseEvent *event) override;
    void mouseReleaseEvent(QMouseEvent *event) override;
    void mouseDoubleClickEvent(QMouseEvent *event) override;
    void keyPressEvent(QKeyEvent *event) override;
private:
    // `stale` tiles stay on screen until their repaint arrives.
    struct Tile { QImage image; QRectF rect; quint64 serial = 0; qint64 bytes = 0; bool stale = false; };
    struct Node { QSGImageNode *node = nullptr; quint64 serial = 0; };
    // Content, links and font changes lay out once, at the next polish.
    void requestRebuild();
    void rebuild();
    size_t layoutKey(qreal width, const QVariantMap &images) const;
    void applyImages();
    void markStale();
    void relayout();
    void applyLayout(bool ok, const QString &failure, const QSizeF &size, qreal scale, HtmlSnapshot snapshot, quint64 generation, size_t key);
    void ensureTiles();
    void dropTiles(bool all);
    void setStatus(const QString &status, const QString &failure = {});
    int linkAt(const QPointF &point, QRectF *box = nullptr) const;
    int offsetAt(const QPointF &point) const;
    void setSelection(int start, int end);
    void findHighlights();
    void updateHover(const QPointF &point);
    QRectF visibleArea(qreal margin) const;
    HtmlImage imageFor(int id, const QVariantMap &value) const;
    std::shared_ptr<HtmlViewState> m_state;
    QVariantMap m_content, m_images;
    QVariantList m_blockedLinks;
    QHash<int, QString> m_alt;
    qreal m_fontPixels = 16;
    QColor m_paper = Qt::white;
    QString m_status, m_failure;
    HtmlSnapshot m_snapshot;
    QSizeF m_documentSize;
    qreal m_scale = 1;
    qreal m_layoutWidth = 0;
    quint64 m_generation = 0, m_paintGeneration = 0, m_tileSerial = 0;
    bool m_layoutPending = false, m_layoutQueued = false, m_imagesQueued = false, m_rebuildQueued = false;
    // The content's hash, and that of the document last laid out.
    size_t m_contentHash = 0, m_laidOutHash = 0;
    QVariantMap m_appliedImages;
    QHash<int, Tile> m_tiles;
    QSet<int> m_pendingTiles;
    QHash<int, Node> m_nodes;
    QSGNode *m_selectionRoot = nullptr, *m_highlightRoot = nullptr;
    bool m_selectionDirty = false;
    QStringList m_highlightPhrases;
    QColor m_highlightColor = QColor(255, 196, 0, 110);
    QList<QPair<int, int>> m_highlights;
    int m_selectionStart = 0, m_selectionEnd = 0, m_anchor = -1, m_pressLink = 0;
    QPointF m_pressPoint;
    bool m_dragging = false;
    int m_hoveredLink = 0;
    QRectF m_hoveredRect;
    QMetaObject::Connection m_frameConnection;
};
