// A sanitised HTML message parsed and laid out by litehtml and painted with
// QPainter, plus the image states and layout snapshot FmHtmlView reads.
#pragma once
#include <QByteArray>
#include <QFont>
#include <QHash>
#include <QImage>
#include <QRectF>
#include <QSet>
#include <QSizeF>
#include <QString>
#include <QVector>
#include <chrono>
#include <memory>

class QPainter;
class HtmlContainer;
namespace litehtml { class document; }

/// How one `fm-res:N` image shows. Anything but Ready is a vector placeholder.
struct HtmlImage {
    /// Hidden: not loaded and not being loaded.
    /// Loading: a fetch is running. Ready: `image` holds pixels. Failed: the
    /// fetch failed. Blocked: the host is on the blocklist. Insecure: a plain
    /// `http:` image that is not fetched.
    enum State { Hidden, Loading, Ready, Failed, Blocked, Insecure };
    State state = Hidden;
    /// Decoded pixels; may be smaller than `size` when memory is short.
    QImage image;
    /// The image's own size in CSS pixels, used for layout.
    QSize size;
    /// The element's `alt` text, shown on the placeholder.
    QString alt;
};

/// What the GUI thread reads after layout, without touching litehtml.
/// All rectangles are in document CSS pixels, origin at the top-left.
struct HtmlSnapshot {
    /// A link hit box; `id` is the N of the `fm-link:N` href. A link that
    /// wraps has one entry per line box.
    struct Link { int id = 0; QRectF rect; };
    /// One drawn text run and the font it was drawn with.
    struct Run { QRectF rect; QString text; QFont font; int offset = 0; };
    QSizeF size;
    QVector<Link> links;
    /// Paint order. `offset` indexes `text`, which joins runs with line breaks.
    QVector<Run> runs;
    /// Every run's text in paint order, for selection, copy and search.
    QString text;
};

/// One parsed sanitised message in CSS pixels. Not thread-safe: the owner runs
/// every call on one thread at a time.
///
/// FmHtmlView creates, lays out and paints documents on its single worker
/// thread. The input is the engine's sanitised HTML, in which images are
/// `fm-res:N` references and links are `fm-link:N`; nothing is fetched and
/// no stylesheet is imported. Fonts map CSS families to installed system
/// fonts.
class HtmlDocument {
public:
    /// Parses `html`. `blockedLinks` are drawn struck through. `images` are
    /// the initial states. `defaultFontPixels` is the CSS `medium` size.
    /// Check valid() before use.
    HtmlDocument(const QByteArray &html, qreal defaultFontPixels, const QSet<int> &blockedLinks,
                 const QHash<int, HtmlImage> &images);
    ~HtmlDocument();
    HtmlDocument(const HtmlDocument &) = delete;
    HtmlDocument &operator=(const HtmlDocument &) = delete;
    /// False when parsing produced no document; every other call is then a
    /// no-op that returns empty values.
    bool valid() const;
    /// Replaces the state of image `id`.
    /// True when the new state can change layout (an undeclared size loaded),
    /// so the owner must call layout() again before painting.
    bool setImage(int id, const HtmlImage &image);
    /// Lays out at `width`; the result is at least that wide.
    /// Content wider than `width`, such as a fixed-width table, makes the
    /// result wider. Must run before snapshot() or paint().
    /// A layout still running at `deadline` stops: it returns an invalid size
    /// and the document becomes invalid.
    QSizeF layout(qreal width, std::chrono::steady_clock::time_point deadline = std::chrono::steady_clock::time_point::max());
    /// Records text runs and link boxes of the current layout.
    HtmlSnapshot snapshot();
    /// Draws the part of the document that intersects `clip` (document CSS
    /// pixels). The caller sets up the painter's transform and device.
    void paint(QPainter &painter, const QRectF &clip);
    /// Decoded image bytes held by every live document.
    /// Process-wide and safe to read from any thread; FmHtmlView uses it to
    /// cap image memory.
    static qint64 liveImageBytes();
private:
    std::unique_ptr<HtmlContainer> m_container;
    std::shared_ptr<litehtml::document> m_document;
};
