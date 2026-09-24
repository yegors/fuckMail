#include "FmHtmlView.h"
#include <QClipboard>
#include <QCoreApplication>
#include <QElapsedTimer>
#include <QFile>
#include <QFontMetricsF>
#include <QFutureWatcher>
#include <QGuiApplication>
#include <QImageReader>
#include <QKeyEvent>
#include <QMutex>
#include <QPainter>
#include <QPointer>
#include <QPromise>
#include <QQuickWindow>
#include <QSGImageNode>
#include <QSGSimpleRectNode>
#include <QThreadPool>
#include <QtQml/qqml.h>
#include <algorithm>
#include <atomic>
#include <cmath>

namespace {
constexpr qreal tileHeight = 512;
constexpr qint64 tileBudget = 32ll * 1024 * 1024;
constexpr qint64 imageBudget = 24ll * 1024 * 1024;
constexpr qint64 maxHtmlBytes = 4ll * 1024 * 1024;
constexpr qint64 layoutLimitMs = 2000;
constexpr qreal minimumScale = 0.5;
std::atomic<qint64> liveTileBytes{0};

// One worker for every HTML view: litehtml documents are not thread-safe,
// and one thread keeps parsing from competing with the GUI.
QThreadPool *htmlPool() {
    static QPointer<QThreadPool> pool;
    if (!pool) {
        pool = new QThreadPool(QCoreApplication::instance());
        pool->setMaxThreadCount(1);
    }
    return pool;
}

template <typename T, typename Work, typename Done>
void runOnPool(QObject *owner, Work work, Done done) {
    QPromise<T> promise;
    auto future = promise.future();
    auto watcher = new QFutureWatcher<T>(owner);
    QObject::connect(watcher, &QFutureWatcher<T>::finished, owner, [watcher, done]() mutable {
        watcher->deleteLater();
        if (watcher->future().resultCount()) done(watcher->future().result());
    });
    watcher->setFuture(future);
    htmlPool()->start([promise = std::move(promise), work = std::move(work)]() mutable {
        promise.start();
        promise.addResult(work());
        promise.finish();
    });
}

HtmlImage::State imageState(const QString &value) {
    if (value == "loading") return HtmlImage::Loading;
    if (value == "ready") return HtmlImage::Ready;
    if (value == "failed") return HtmlImage::Failed;
    if (value == "blocked") return HtmlImage::Blocked;
    if (value == "insecure") return HtmlImage::Insecure;
    return HtmlImage::Hidden;
}

// Decodes a verified engine PNG. When images already use the budget, pixels
// are halved while the layout keeps the image's own size.
QImage decode(const QString &path, QSize *natural) {
    QImageReader reader(path, "png");
    reader.setAutoTransform(false);
    const QSize size = reader.size();
    if (!size.isValid() || size.width() > 4096 || size.height() > 16384) return {};
    *natural = size;
    const qint64 bytes = qint64(size.width()) * size.height() * 4;
    const qint64 used = HtmlDocument::liveImageBytes();
    if (used + bytes > 4 * imageBudget) return {};
    if (used + bytes > imageBudget) reader.setScaledSize(QSize(std::max(1, size.width() / 2), std::max(1, size.height() / 2)));
    QImage image = reader.read();
    return image.isNull() ? image : image.convertToFormat(QImage::Format_ARGB32_Premultiplied);
}

// Heights of recent layouts by content, width, font, blocked links and image
// states. A message shown again takes its final height on its first frame
// while the worker lays it out. GUI thread only.
constexpr int rememberedHeights = 128;
struct HeightMemory { QHash<size_t, qreal> heights; QList<size_t> order; };
HeightMemory &heightMemory() {
    static HeightMemory memory;
    return memory;
}
void rememberHeight(size_t key, qreal height) {
    auto &memory = heightMemory();
    if (!memory.heights.contains(key)) {
        memory.order.append(key);
        while (memory.order.size() > rememberedHeights) memory.heights.remove(memory.order.takeFirst());
    }
    memory.heights.insert(key, height);
}

struct LayoutResult {
    bool ok = false;
    QString failure;
    QSizeF size;
    qreal scale = 1;
    HtmlSnapshot snapshot;
    bool relaid = false;
};

// Wider fixed layouts scale to fit, down to half size; beyond that they clip.
LayoutResult layoutAt(HtmlDocument &document, qreal width) {
    LayoutResult result;
    QSizeF size = document.layout(width);
    qreal scale = 1;
    if (size.width() > width + 0.5) {
        scale = std::max(minimumScale, width / size.width());
        size = document.layout(std::floor(width / scale));
    }
    result.ok = size.height() >= 0;
    result.size = size;
    result.scale = scale;
    result.snapshot = document.snapshot();
    result.relaid = true;
    return result;
}
}

struct HtmlViewState {
    QMutex mutex;
    std::unique_ptr<HtmlDocument> document;
};

FmHtmlView::FmHtmlView(QQuickItem *parent) : QQuickItem(parent), m_state(std::make_shared<HtmlViewState>()) {
    setFlag(ItemHasContents);
    setAcceptHoverEvents(true);
    setAcceptedMouseButtons(Qt::LeftButton);
    setActiveFocusOnTab(false);
}

FmHtmlView::~FmHtmlView() {
    for (const auto &tile : std::as_const(m_tiles)) liveTileBytes -= tile.bytes;
    // Large documents are freed on the worker, after any queued job.
    if (!QCoreApplication::instance()) return;
    htmlPool()->start([state = std::move(m_state)]() mutable {
        QMutexLocker lock(&state->mutex);
        state->document.reset();
    });
}

void FmHtmlView::registerType() {
    static const int type = qmlRegisterType<FmHtmlView>("FuckMail.Html", 1, 0, "FmHtmlView");
    Q_UNUSED(type)
}

qint64 FmHtmlView::tileBytes() { return liveTileBytes; }

void FmHtmlView::setContent(const QVariantMap &content) {
    if (content == m_content) return;
    m_content = content;
    m_contentHash = content.isEmpty() ? 0 : qHashMulti(0, content.value("html").toString(), content.value("html_path").toString());
    m_alt.clear();
    for (const auto &value : content.value("resources").toList()) {
        const auto resource = value.toMap();
        m_alt.insert(resource.value("id").toInt(), resource.value("alt").toString());
    }
    emit contentChanged();
    requestRebuild();
}

void FmHtmlView::setImages(const QVariantMap &images) {
    if (images == m_images) return;
    m_images = images;
    emit imagesChanged();
    if (m_status != "ready" || m_imagesQueued) return;
    m_imagesQueued = true;
    // Several image results arriving together cost one relayout.
    QMetaObject::invokeMethod(this, &FmHtmlView::applyImages, Qt::QueuedConnection);
}

void FmHtmlView::applyImages() {
    m_imagesQueued = false;
    if (m_status != "ready" || m_images == m_appliedImages) return;
    QHash<int, QVariantMap> changed;
    for (auto it = m_images.cbegin(); it != m_images.cend(); ++it)
        if (m_appliedImages.value(it.key()) != it.value()) changed.insert(it.key().toInt(), it.value().toMap());
    for (auto it = m_appliedImages.cbegin(); it != m_appliedImages.cend(); ++it)
        if (!m_images.contains(it.key())) changed.insert(it.key().toInt(), {});
    m_appliedImages = m_images;
    QHash<int, HtmlImage> states;
    for (auto it = changed.cbegin(); it != changed.cend(); ++it) states.insert(it.key(), imageFor(it.key(), it.value()));
    const auto state = m_state;
    const qreal width = m_layoutWidth;
    const quint64 generation = m_generation;
    const size_t key = layoutKey(width, m_images);
    runOnPool<LayoutResult>(this, [state, states, changed, width]() mutable {
        QMutexLocker lock(&state->mutex);
        LayoutResult result;
        if (!state->document) return result;
        bool relayout = false;
        for (auto it = states.begin(); it != states.end(); ++it) {
            auto image = it.value();
            if (image.state == HtmlImage::Ready) {
                image.image = decode(changed.value(it.key()).value("path").toString(), &image.size);
                if (image.image.isNull()) image.state = HtmlImage::Failed;
            }
            relayout = state->document->setImage(it.key(), image) || relayout;
        }
        result.ok = true;
        if (relayout && width > 0) result = layoutAt(*state->document, width);
        return result;
    }, [this, generation, key](LayoutResult result) {
        if (generation != m_generation) return;
        if (result.relaid) applyLayout(result.ok, result.failure, result.size, result.scale, std::move(result.snapshot), generation, key);
        else { rememberHeight(key, implicitHeight()); markStale(); ensureTiles(); }
    });
}

void FmHtmlView::markStale() {
    ++m_paintGeneration;
    for (auto &tile : m_tiles) tile.stale = true;
    m_pendingTiles.clear();
}

void FmHtmlView::setBlockedLinks(const QVariantList &links) {
    if (links == m_blockedLinks) return;
    m_blockedLinks = links;
    emit blockedLinksChanged();
    if (!m_content.isEmpty()) requestRebuild();
}

void FmHtmlView::setFontPixels(qreal pixels) {
    if (qFuzzyCompare(pixels, m_fontPixels) || pixels <= 0) return;
    m_fontPixels = pixels;
    emit fontPixelsChanged();
    if (!m_content.isEmpty()) requestRebuild();
}

void FmHtmlView::setPaper(const QColor &paper) {
    if (paper == m_paper) return;
    m_paper = paper;
    emit paperChanged();
    markStale();
    ensureTiles();
}

HtmlImage FmHtmlView::imageFor(int id, const QVariantMap &value) const {
    HtmlImage image;
    image.state = imageState(value.value("state").toString());
    image.alt = m_alt.value(id);
    return image;
}

void FmHtmlView::setStatus(const QString &status, const QString &failure) {
    if (status == m_status && failure == m_failure) return;
    m_status = status; m_failure = failure;
    emit statusChanged();
}

void FmHtmlView::requestRebuild() {
    // Results for the previous content or links are dropped from now on.
    ++m_generation;
    m_rebuildQueued = true;
    setStatus(m_content.isEmpty() ? QString() : QStringLiteral("loading"));
    polish();
}

void FmHtmlView::updatePolish() {
    if (m_rebuildQueued) rebuild();
}

size_t FmHtmlView::layoutKey(qreal width, const QVariantMap &images) const {
    QString states, blocked;
    for (auto it = images.cbegin(); it != images.cend(); ++it) {
        const auto value = it.value().toMap();
        states += it.key() + ':' + value.value("state").toString() + ':' + value.value("path").toString() + ';';
    }
    for (const auto &value : m_blockedLinks) blocked += QString::number(value.toInt()) + ',';
    return qHashMulti(0, m_contentHash, qRound(width * 4), qRound(m_fontPixels * 4), states, blocked);
}

void FmHtmlView::rebuild() {
    ++m_generation;
    m_layoutPending = false;
    m_rebuildQueued = false;
    // The same document with other links or fonts keeps its tiles and height
    // until the new layout paints; other content starts empty.
    const bool sameContent = m_contentHash && m_contentHash == m_laidOutHash;
    if (sameContent) markStale();
    else dropTiles(true);
    const bool hadText = !m_snapshot.text.isEmpty();
    m_snapshot = {}; m_documentSize = {}; m_selectionStart = m_selectionEnd = 0; m_anchor = -1;
    m_hoveredLink = 0; m_hoveredRect = {};
    if (hadText) emit textChanged();
    emit selectionChanged(); emit hoveredLinkChanged();
    if (m_content.isEmpty()) { setImplicitHeight(0); setStatus({}); return; }
    if (width() <= 0) { setStatus("loading"); m_layoutPending = true; return; }
    setStatus("loading");
    const size_t key = layoutKey(width(), m_images);
    if (const auto known = heightMemory().heights.constFind(key); known != heightMemory().heights.cend()) setImplicitHeight(*known);
    else if (!sameContent) setImplicitHeight(0);
    QSet<int> blocked;
    for (const auto &value : m_blockedLinks) blocked.insert(value.toInt());
    QHash<int, HtmlImage> images;
    QHash<int, QString> paths;
    for (auto it = m_images.cbegin(); it != m_images.cend(); ++it) {
        const auto value = it.value().toMap();
        images.insert(it.key().toInt(), imageFor(it.key().toInt(), value));
        if (value.value("state") == "ready") paths.insert(it.key().toInt(), value.value("path").toString());
    }
    for (auto it = m_alt.cbegin(); it != m_alt.cend(); ++it)
        if (!images.contains(it.key())) { HtmlImage image; image.alt = it.value(); images.insert(it.key(), image); }
    m_appliedImages = m_images;
    const auto html = m_content.value("html").toString().toUtf8();
    const auto path = m_content.value("html_path").toString();
    const auto state = m_state;
    const qreal width = this->width(), font = m_fontPixels;
    m_layoutWidth = width;
    const quint64 generation = m_generation;
    runOnPool<LayoutResult>(this, [state, html, path, width, font, blocked, images, paths]() mutable {
        QElapsedTimer timer; timer.start();
        LayoutResult result;
        QByteArray source = html;
        if (source.isEmpty() && !path.isEmpty()) {
            QFile file(path);
            if (!file.open(QIODevice::ReadOnly) || file.size() > maxHtmlBytes) { result.failure = "This message's HTML could not be read."; return result; }
            source = file.readAll();
        }
        for (auto it = paths.cbegin(); it != paths.cend(); ++it) {
            auto &image = images[it.key()];
            image.image = decode(it.value(), &image.size);
            if (image.image.isNull()) image.state = HtmlImage::Failed;
        }
        auto document = std::make_unique<HtmlDocument>(source, font, blocked, images);
        if (!document->valid()) { result.failure = "This message's HTML could not be laid out."; return result; }
        result = layoutAt(*document, width);
        if (timer.elapsed() > layoutLimitMs) { result.ok = false; result.failure = "This message's HTML took too long to lay out."; return result; }
        QMutexLocker lock(&state->mutex);
        state->document = std::move(document);
        return result;
    }, [this, generation, key](LayoutResult result) {
        applyLayout(result.ok, result.failure, result.size, result.scale, std::move(result.snapshot), generation, key);
    });
}

void FmHtmlView::relayout() {
    if (!m_state || m_status != "ready" || width() <= 0) return;
    if (m_layoutQueued) return;
    m_layoutQueued = true;
    QMetaObject::invokeMethod(this, [this] {
        m_layoutQueued = false;
        if (qFuzzyCompare(width(), m_layoutWidth) || width() <= 0) return;
        m_layoutWidth = width();
        const auto state = m_state;
        const qreal width = m_layoutWidth;
        const quint64 generation = m_generation;
        const size_t key = layoutKey(width, m_appliedImages);
        runOnPool<LayoutResult>(this, [state, width] {
            QMutexLocker lock(&state->mutex);
            return state->document ? layoutAt(*state->document, width) : LayoutResult{};
        }, [this, generation, key](LayoutResult result) {
            applyLayout(result.ok, result.failure, result.size, result.scale, std::move(result.snapshot), generation, key);
        });
    }, Qt::QueuedConnection);
}

void FmHtmlView::applyLayout(bool ok, const QString &failure, const QSizeF &size, qreal scale, HtmlSnapshot snapshot, quint64 generation, size_t key) {
    if (generation != m_generation) return;
    if (!ok) { m_laidOutHash = 0; dropTiles(true); setImplicitHeight(0); setStatus("failed", failure.isEmpty() ? QStringLiteral("This message's HTML could not be shown.") : failure); return; }
    m_laidOutHash = m_contentHash;
    // Old tiles stay visible until their replacements are painted.
    markStale();
    const bool textChangedNow = snapshot.text != m_snapshot.text;
    m_snapshot = std::move(snapshot);
    m_documentSize = size;
    const bool scaleChanged = !qFuzzyCompare(scale, m_scale);
    m_scale = scale;
    m_selectionStart = std::min(m_selectionStart, int(m_snapshot.text.size()));
    m_selectionEnd = std::min(m_selectionEnd, int(m_snapshot.text.size()));
    m_selectionDirty = true;
    setImplicitHeight(std::ceil(size.height() * scale));
    rememberHeight(key, implicitHeight());
    setStatus("ready");
    if (textChangedNow) emit textChanged();
    Q_UNUSED(scaleChanged)
    emit layoutChanged();
    if (!qFuzzyCompare(width(), m_layoutWidth)) relayout();
    if (m_images != m_appliedImages && !m_imagesQueued) { m_imagesQueued = true; QMetaObject::invokeMethod(this, &FmHtmlView::applyImages, Qt::QueuedConnection); }
    ensureTiles();
    update();
}

QRectF FmHtmlView::visibleArea(qreal margin) const {
    if (!window() || !isVisible() || width() <= 0 || height() <= 0) return {};
    QRectF clip(0, 0, window()->width(), window()->height());
    for (auto *item = parentItem(); item; item = item->parentItem())
        if (item->clip()) clip &= item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
    if (clip.isEmpty()) return {};
    clip.adjust(0, -margin * clip.height(), 0, margin * clip.height());
    const QRectF area = mapRectToScene(QRectF(0, 0, width(), height())) & clip;
    return area.isEmpty() ? QRectF() : mapRectFromScene(area);
}

void FmHtmlView::ensureTiles() {
    if (m_status != "ready" || !m_state || !window()) return;
    const QRectF visible = visibleArea(0);
    const QRectF keep = visibleArea(1);
    const qreal total = std::ceil(m_documentSize.height() * m_scale);
    const qreal dpr = window()->effectiveDevicePixelRatio();
    const qreal tileWidth = width();
    // Tiles off screen by more than a screen are released.
    for (auto it = m_tiles.begin(); it != m_tiles.end();) {
        const QRectF area(0, it.key() * tileHeight, tileWidth, tileHeight);
        if (keep.isEmpty() || !area.intersects(keep) || it.key() * tileHeight >= total) { liveTileBytes -= it->bytes; it = m_tiles.erase(it); update(); }
        else ++it;
    }
    if (keep.isEmpty() || total <= 0) return;
    const qint64 bytesPerTile = qint64(std::ceil(tileWidth * dpr)) * qint64(std::ceil(tileHeight * dpr)) * 4;
    QList<int> wanted;
    const auto add = [&](const QRectF &area, bool margin) {
        if (area.isEmpty()) return;
        const int first = std::max(0, int(std::floor(area.top() / tileHeight)));
        const int last = std::min(int(std::ceil(total / tileHeight)) - 1, int(std::floor(area.bottom() / tileHeight)));
        for (int index = first; index <= last; ++index) {
            if (wanted.contains(index)) continue;
            const auto tile = m_tiles.constFind(index);
            const bool current = tile != m_tiles.cend() && tile->serial != 0 && !tile->stale;
            if (current || m_pendingTiles.contains(index)) continue;
            if (margin && liveTileBytes + bytesPerTile > tileBudget) continue;
            wanted.append(index);
        }
    };
    add(visible, false);
    add(keep, true);
    const auto state = m_state;
    const quint64 generation = m_generation, paint = m_paintGeneration;
    const qreal scale = m_scale;
    const QColor paper = m_paper;
    for (const int index : wanted) {
        m_pendingTiles.insert(index);
        const qreal top = index * tileHeight, height = std::min(tileHeight, total - top);
        runOnPool<QImage>(this, [state, top, height, tileWidth, dpr, scale, paper] {
            QImage image(QSize(int(std::ceil(tileWidth * dpr)), int(std::ceil(height * dpr))), QImage::Format_ARGB32_Premultiplied);
            image.setDevicePixelRatio(dpr);
            image.fill(paper);
            QMutexLocker lock(&state->mutex);
            if (!state->document) return QImage();
            QPainter painter(&image);
            painter.setRenderHint(QPainter::TextAntialiasing);
            painter.scale(scale, scale);
            painter.translate(0, -top / scale);
            const QRectF clip(0, top / scale, tileWidth / scale, height / scale);
            painter.setClipRect(clip);
            state->document->paint(painter, clip);
            return image;
        }, [this, index, generation, paint, top, height, tileWidth](QImage image) {
            m_pendingTiles.remove(index);
            if (generation != m_generation || paint != m_paintGeneration || image.isNull()) {
                if (generation == m_generation) ensureTiles();
                return;
            }
            auto &tile = m_tiles[index];
            liveTileBytes -= tile.bytes;
            tile.image = std::move(image);
            tile.rect = QRectF(0, top, tileWidth, height);
            tile.serial = ++m_tileSerial;
            tile.stale = false;
            tile.bytes = tile.image.sizeInBytes();
            liveTileBytes += tile.bytes;
            update();
        });
    }
}

void FmHtmlView::dropTiles(bool all) {
    Q_UNUSED(all)
    for (const auto &tile : std::as_const(m_tiles)) liveTileBytes -= tile.bytes;
    m_tiles.clear();
    m_pendingTiles.clear();
    update();
}

QSGNode *FmHtmlView::updatePaintNode(QSGNode *old, UpdatePaintNodeData *) {
    auto *root = old;
    if (!root) {
        // A transform node keeps its scene position current in the software
        // renderer, which places later children from their parent's state.
        root = new QSGTransformNode;
        m_nodes.clear();
        m_selectionRoot = nullptr;
        // Earlier textures went with the old tree; repaint the kept tiles.
        bool lost = false;
        for (auto it = m_tiles.begin(); it != m_tiles.end();) {
            if (it->image.isNull()) { liveTileBytes -= it->bytes; it = m_tiles.erase(it); lost = true; }
            else ++it;
        }
        if (lost) QMetaObject::invokeMethod(this, &FmHtmlView::ensureTiles, Qt::QueuedConnection);
    }
    if (!m_selectionRoot) { m_selectionRoot = new QSGNode; root->appendChildNode(m_selectionRoot); m_selectionDirty = true; }
    for (auto it = m_nodes.begin(); it != m_nodes.end();) {
        const auto tile = m_tiles.constFind(it.key());
        if (tile == m_tiles.cend() || tile->serial != it->serial) {
            root->removeChildNode(it->node); delete it->node; it = m_nodes.erase(it);
        } else ++it;
    }
    for (auto it = m_tiles.begin(); it != m_tiles.end(); ++it) {
        if (m_nodes.contains(it.key()) || it->image.isNull() || !window()) continue;
        auto *node = window()->createImageNode();
        node->setTexture(window()->createTextureFromImage(it->image));
        node->setOwnsTexture(true);
        node->setFiltering(QSGTexture::Linear);
        node->setRect(it->rect);
        root->insertChildNodeBefore(node, m_selectionRoot);
        m_nodes.insert(it.key(), {node, it->serial});
        // The texture holds the pixels now.
        it->image = QImage();
    }
    if (m_selectionDirty) {
        m_selectionDirty = false;
        while (auto *child = m_selectionRoot->firstChild()) { m_selectionRoot->removeChildNode(child); delete child; }
        if (hasSelection()) {
            const QColor highlight(51, 144, 255, 90);
            for (const auto &run : std::as_const(m_snapshot.runs)) {
                const int start = run.offset, end = run.offset + int(run.text.size());
                if (end <= m_selectionStart || start >= m_selectionEnd) continue;
                const QFontMetricsF metrics(run.font);
                const int from = std::max(0, m_selectionStart - start), to = std::min(int(run.text.size()), m_selectionEnd - start);
                const qreal left = run.rect.left() + metrics.horizontalAdvance(run.text.left(from));
                const qreal right = run.rect.left() + metrics.horizontalAdvance(run.text.left(to));
                const QRectF box(left * m_scale, run.rect.top() * m_scale, (right - left) * m_scale, run.rect.height() * m_scale);
                if (box.width() > 0) m_selectionRoot->appendChildNode(new QSGSimpleRectNode(box, highlight));
            }
        }
    }
    return root;
}

void FmHtmlView::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (qFuzzyCompare(newGeometry.width(), oldGeometry.width())) { ensureTiles(); return; }
    if (m_layoutPending && newGeometry.width() > 0) { rebuild(); return; }
    relayout();
}

void FmHtmlView::itemChange(ItemChange change, const ItemChangeData &data) {
    if (change == ItemSceneChange) {
        QObject::disconnect(m_frameConnection);
        dropTiles(true);
        if (data.window)
            // Each frame, keep the tiles near the viewport current. Scrolling
            // moves existing textures; only new tiles are painted.
            m_frameConnection = connect(data.window, &QQuickWindow::afterAnimating, this, &FmHtmlView::ensureTiles);
    } else if (change == ItemVisibleHasChanged) {
        if (!data.boolValue) dropTiles(true);
        else ensureTiles();
    }
    QQuickItem::itemChange(change, data);
}

void FmHtmlView::releaseResources() {
    m_nodes.clear();
    m_selectionRoot = nullptr;
}

int FmHtmlView::linkAt(const QPointF &point, QRectF *box) const {
    const QPointF at(point.x() / m_scale, point.y() / m_scale);
    for (const auto &link : m_snapshot.links)
        if (link.rect.adjusted(-1, -1, 1, 1).contains(at)) {
            if (box) *box = QRectF(link.rect.x() * m_scale, link.rect.y() * m_scale, link.rect.width() * m_scale, link.rect.height() * m_scale);
            return link.id;
        }
    return 0;
}

int FmHtmlView::offsetAt(const QPointF &point) const {
    if (m_snapshot.runs.isEmpty()) return 0;
    const QPointF at(point.x() / m_scale, point.y() / m_scale);
    const HtmlSnapshot::Run *best = nullptr;
    qreal bestScore = 0;
    for (const auto &run : m_snapshot.runs) {
        const qreal dy = at.y() < run.rect.top() ? run.rect.top() - at.y() : at.y() > run.rect.bottom() ? at.y() - run.rect.bottom() : 0;
        const qreal dx = at.x() < run.rect.left() ? run.rect.left() - at.x() : at.x() > run.rect.right() ? at.x() - run.rect.right() : 0;
        // Same-line runs win over nearer runs on other lines.
        const qreal score = dy * 10000 + dx;
        if (!best || score < bestScore) { best = &run; bestScore = score; }
        if (score == 0) break;
    }
    const QFontMetricsF metrics(best->font);
    const qreal x = at.x() - best->rect.left();
    int index = 0;
    for (; index < best->text.size(); ++index) {
        const qreal before = metrics.horizontalAdvance(best->text.left(index));
        const qreal after = metrics.horizontalAdvance(best->text.left(index + 1));
        if (x < (before + after) / 2) break;
    }
    return best->offset + index;
}

void FmHtmlView::setSelection(int start, int end) {
    if (start > end) std::swap(start, end);
    start = std::clamp(start, 0, int(m_snapshot.text.size()));
    end = std::clamp(end, 0, int(m_snapshot.text.size()));
    if (start == m_selectionStart && end == m_selectionEnd) return;
    m_selectionStart = start; m_selectionEnd = end;
    m_selectionDirty = true;
    update();
    emit selectionChanged();
}

QString FmHtmlView::selectedText() const { return m_snapshot.text.mid(m_selectionStart, m_selectionEnd - m_selectionStart); }
void FmHtmlView::copySelection() const {
    if (hasSelection()) QGuiApplication::clipboard()->setText(selectedText());
}
void FmHtmlView::selectAll() { setSelection(0, int(m_snapshot.text.size())); }
void FmHtmlView::clearSelection() { setSelection(0, 0); }

QVariantList FmHtmlView::linkRects(int id) const {
    QVariantList rects;
    for (const auto &link : m_snapshot.links)
        if (link.id == id) rects.append(QRectF(link.rect.x() * m_scale, link.rect.y() * m_scale, link.rect.width() * m_scale, link.rect.height() * m_scale));
    return rects;
}

QVariantMap FmHtmlView::debugState() const {
    int textures = 0;
    for (const auto &tile : m_tiles) textures += tile.serial != 0;
    return {{"status", m_status}, {"scale", m_scale}, {"document_width", m_documentSize.width()}, {"document_height", m_documentSize.height()},
            {"runs", int(m_snapshot.runs.size())}, {"links", int(m_snapshot.links.size())}, {"tiles", textures},
            {"pending_tiles", int(m_pendingTiles.size())}, {"tile_bytes", tileBytes()}, {"image_bytes", HtmlDocument::liveImageBytes()}};
}

void FmHtmlView::updateHover(const QPointF &point) {
    QRectF box;
    const int link = linkAt(point, &box);
    setCursor(link ? Qt::PointingHandCursor : Qt::IBeamCursor);
    if (link == m_hoveredLink && box == m_hoveredRect) return;
    m_hoveredLink = link; m_hoveredRect = box;
    emit hoveredLinkChanged();
}

void FmHtmlView::hoverMoveEvent(QHoverEvent *event) { updateHover(event->position()); }
void FmHtmlView::hoverLeaveEvent(QHoverEvent *) {
    if (!m_hoveredLink) return;
    m_hoveredLink = 0; m_hoveredRect = {};
    emit hoveredLinkChanged();
}

void FmHtmlView::mousePressEvent(QMouseEvent *event) {
    forceActiveFocus(Qt::MouseFocusReason);
    // Dragging selects text; the reader's Flickable must not take the drag.
    setKeepMouseGrab(true);
    m_pressPoint = event->position();
    m_pressLink = linkAt(m_pressPoint);
    m_dragging = false;
    m_anchor = offsetAt(m_pressPoint);
    if (event->modifiers() & Qt::ShiftModifier && hasSelection()) setSelection(m_selectionStart, m_anchor);
    else clearSelection();
    event->accept();
}

void FmHtmlView::mouseMoveEvent(QMouseEvent *event) {
    if (m_anchor < 0) return;
    if (!m_dragging && (event->position() - m_pressPoint).manhattanLength() < 4) return;
    m_dragging = true;
    setSelection(m_anchor, offsetAt(event->position()));
}

void FmHtmlView::mouseReleaseEvent(QMouseEvent *event) {
    const int link = m_pressLink;
    const bool click = !m_dragging;
    m_anchor = -1; m_pressLink = 0; m_dragging = false;
    setKeepMouseGrab(false);
    if (click && link && linkAt(event->position()) == link) emit linkActivated(link);
}

void FmHtmlView::mouseDoubleClickEvent(QMouseEvent *event) {
    const int at = offsetAt(event->position());
    const auto &text = m_snapshot.text;
    int start = at, end = at;
    while (start > 0 && text.at(start - 1).isLetterOrNumber()) --start;
    while (end < text.size() && text.at(end).isLetterOrNumber()) ++end;
    setSelection(start, end);
    m_anchor = -1;
}

void FmHtmlView::keyPressEvent(QKeyEvent *event) {
    if (event->matches(QKeySequence::Copy) && hasSelection()) { copySelection(); event->accept(); return; }
    if (event->matches(QKeySequence::SelectAll)) { selectAll(); event->accept(); return; }
    QQuickItem::keyPressEvent(event);
}
