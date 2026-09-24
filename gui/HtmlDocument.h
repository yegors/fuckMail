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
#include <memory>

class QPainter;
class HtmlContainer;
namespace litehtml { class document; }

// How one `fm-res:N` image shows. Anything but Ready is a vector placeholder.
struct HtmlImage {
    enum State { Hidden, Loading, Ready, Failed, Blocked, Insecure };
    State state = Hidden;
    // Decoded pixels; may be smaller than `size` when memory is short.
    QImage image;
    // The image's own size in CSS pixels, used for layout.
    QSize size;
    QString alt;
};

// What the GUI thread reads after layout, without touching litehtml.
struct HtmlSnapshot {
    struct Link { int id = 0; QRectF rect; };
    struct Run { QRectF rect; QString text; QFont font; int offset = 0; };
    QSizeF size;
    QVector<Link> links;
    // Paint order. `offset` indexes `text`, which joins runs with line breaks.
    QVector<Run> runs;
    QString text;
};

// One parsed sanitised message in CSS pixels. Not thread-safe: the owner runs
// every call on one thread at a time.
class HtmlDocument {
public:
    // `blockedLinks` are drawn struck through. `images` are the initial states.
    HtmlDocument(const QByteArray &html, qreal defaultFontPixels, const QSet<int> &blockedLinks,
                 const QHash<int, HtmlImage> &images);
    ~HtmlDocument();
    HtmlDocument(const HtmlDocument &) = delete;
    HtmlDocument &operator=(const HtmlDocument &) = delete;
    bool valid() const;
    // True when the new state can change layout (an undeclared size loaded).
    bool setImage(int id, const HtmlImage &image);
    // Lays out at `width`; the result is at least that wide.
    QSizeF layout(qreal width);
    HtmlSnapshot snapshot();
    void paint(QPainter &painter, const QRectF &clip);
    // Decoded image bytes held by every live document.
    static qint64 liveImageBytes();
private:
    std::unique_ptr<HtmlContainer> m_container;
    std::shared_ptr<litehtml::document> m_document;
};
