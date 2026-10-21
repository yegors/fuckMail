// HtmlDocument on top of the vendored litehtml: HtmlContainer implements
// litehtml's document_container with Qt fonts and QPainter drawing, including
// image placeholders and blocked-link styling.
#include "HtmlDocument.h"
#include <litehtml.h>
#include <litehtml/render_item.h>
#include <QConicalGradient>
#include <QFontDatabase>
#include <QFontMetricsF>
#include <QLinearGradient>
#include <QMutex>
#include <QPainter>
#include <QPainterPath>
#include <QRadialGradient>
#include <algorithm>
#include <atomic>
#include <cmath>
#include <cstring>
#include <functional>

namespace {
std::atomic<qint64> liveBytes{0};
constexpr qreal chipHeight = 24, chipMaxWidth = 240, chipMinWidth = 96;
// Largest font and document size passed on. litehtml keeps computed font
// sizes within the font bound; the document bound keeps a hostile layout's
// size finite and small enough for the int tile and pixel arithmetic of
// the view, far beyond any real message.
constexpr float maxFontPixels = 1000;
constexpr qreal maxDocumentPixels = 4'000'000;
// Largest scaled copy of a background tile, in pixels (16 MB).
constexpr qint64 maxTilePixels = 4'000'000;

QSizeF clampedSize(const QSizeF &size) {
    const auto bound = [](qreal value) { return std::isfinite(value) ? std::clamp(value, qreal(0), maxDocumentPixels) : maxDocumentPixels; };
    return QSizeF(bound(size.width()), bound(size.height()));
}

int prefixedId(const char *value, const char *prefix) {
    if (!value) return 0;
    const auto length = std::strlen(prefix);
    if (std::strncmp(value, prefix, length) != 0) return 0;
    bool ok = false;
    const int id = QByteArray(value + length).toInt(&ok);
    return ok && id > 0 ? id : 0;
}
int resourceId(const std::string &url) { return prefixedId(url.c_str(), "fm-res:"); }

QColor color(const litehtml::web_color &c) { return QColor(c.red, c.green, c.blue, c.alpha); }
QRectF rect(const litehtml::position &p) { return QRectF(float(p.x), float(p.y), float(p.width), float(p.height)); }

QPainterPath rounded(const QRectF &r, const litehtml::border_radiuses &b) {
    QPainterPath path;
    const qreal tlx = float(b.top_left_x), tly = float(b.top_left_y), trx = float(b.top_right_x), try_ = float(b.top_right_y);
    const qreal brx = float(b.bottom_right_x), bry = float(b.bottom_right_y), blx = float(b.bottom_left_x), bly = float(b.bottom_left_y);
    if (tlx <= 0 && tly <= 0 && trx <= 0 && try_ <= 0 && brx <= 0 && bry <= 0 && blx <= 0 && bly <= 0) { path.addRect(r); return path; }
    path.moveTo(r.left() + tlx, r.top());
    path.lineTo(r.right() - trx, r.top());
    if (trx > 0 && try_ > 0) path.arcTo(QRectF(r.right() - 2 * trx, r.top(), 2 * trx, 2 * try_), 90, -90);
    path.lineTo(r.right(), r.bottom() - bry);
    if (brx > 0 && bry > 0) path.arcTo(QRectF(r.right() - 2 * brx, r.bottom() - 2 * bry, 2 * brx, 2 * bry), 0, -90);
    path.lineTo(r.left() + blx, r.bottom());
    if (blx > 0 && bly > 0) path.arcTo(QRectF(r.left(), r.bottom() - 2 * bly, 2 * blx, 2 * bly), 270, -90);
    path.lineTo(r.left(), r.top() + tly);
    if (tlx > 0 && tly > 0) path.arcTo(QRectF(r.left(), r.top(), 2 * tlx, 2 * tly), 180, -90);
    path.closeSubpath();
    return path;
}

// CSS family lists map to installed system fonts. No web fonts load.
QString firstInstalled(std::initializer_list<const char *> names, QFontDatabase::SystemFont fallback) {
    const auto families = QFontDatabase::families();
    for (const auto name : names)
        if (families.contains(QLatin1String(name), Qt::CaseInsensitive)) return QString::fromLatin1(name);
    return QFontDatabase::systemFont(fallback).family();
}
const QString &sansFamily() {
    static const QString family = firstInstalled({"Arial", "Helvetica", "Liberation Sans", "DejaVu Sans"}, QFontDatabase::GeneralFont);
    return family;
}
const QString &serifFamily() {
    static const QString family = firstInstalled({"Georgia", "Times New Roman", "Times", "Liberation Serif", "DejaVu Serif"}, QFontDatabase::GeneralFont);
    return family;
}
const QString &monoFamily() {
    static const QString family = firstInstalled({"Consolas", "Menlo", "SF Mono", "DejaVu Sans Mono", "Liberation Mono", "Courier New"}, QFontDatabase::FixedFont);
    return family;
}
// Resolves a CSS font-family list to one installed family. The cache is shared
// by every document, whatever thread builds it, hence the mutex; it is cleared
// past 512 entries so the family lists found in mail cannot grow it unbounded.
QString resolveFamily(const std::string &list) {
    static QMutex mutex;
    static QHash<QString, QString> resolved;
    const auto key = QString::fromStdString(list);
    QMutexLocker lock(&mutex);
    if (const auto found = resolved.constFind(key); found != resolved.cend()) return *found;
    if (resolved.size() > 512) resolved.clear();
    QString result;
    for (auto name : key.split(',')) {
        name = name.trimmed();
        if (name.size() >= 2 && (name.front() == '"' || name.front() == '\'') && name.back() == name.front()) name = name.mid(1, name.size() - 2).trimmed();
        const auto lower = name.toLower();
        if (lower.isEmpty()) continue;
        if (lower == "sans-serif" || lower == "system-ui" || lower == "-apple-system" || lower == "blinkmacsystemfont" || lower == "cursive" || lower == "fantasy") { result = sansFamily(); break; }
        if (lower == "serif") { result = serifFamily(); break; }
        if (lower == "monospace") { result = monoFamily(); break; }
        if (QFontDatabase::hasFamily(name)) { result = name; break; }
    }
    if (result.isEmpty()) result = sansFamily();
    resolved.insert(key, result);
    return result;
}

struct Font {
    QFont font;
    QFontMetricsF metrics;
};

// The `hdc` handle litehtml passes back to every draw callback. With `record`
// set, a draw pass collects text runs for snapshot() and paints nothing.
struct Canvas {
    QPainter *painter = nullptr;
    HtmlSnapshot *record = nullptr;
};

const QImage &stripes() {
    // The design's 135° stripes: 8 px bands of two pale greys.
    static const QImage tile = [] {
        QImage image(16, 16, QImage::Format_ARGB32_Premultiplied);
        image.fill(QColor(0xf4, 0xf5, 0xf7));
        QPainter painter(&image);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setPen(Qt::NoPen); painter.setBrush(QColor(0xee, 0xf0, 0xf3));
        for (int offset = -32; offset <= 32; offset += 16) {
            const QPolygonF band{{qreal(offset), 0}, {qreal(offset + 8), 0}, {qreal(offset + 24), 16}, {qreal(offset + 16), 16}};
            painter.drawPolygon(band);
        }
        return image;
    }();
    return tile;
}

void placeholderIcon(QPainter &painter, const QRectF &box, HtmlImage::State state, const QColor &ink) {
    painter.save();
    painter.translate(box.topLeft());
    painter.scale(box.width() / 16, box.height() / 16);
    QPen pen(ink, 1.4); pen.setCapStyle(Qt::RoundCap); pen.setJoinStyle(Qt::RoundJoin);
    painter.setPen(pen); painter.setBrush(Qt::NoBrush);
    if (state == HtmlImage::Blocked || state == HtmlImage::Insecure) {
        painter.drawEllipse(QPointF(8, 8), 5.5, 5.5);
        painter.drawLine(QPointF(4.1, 11.9), QPointF(11.9, 4.1));
    } else {
        painter.drawRect(QRectF(2.5, 3.5, 11, 9));
        painter.drawPolyline(QPolygonF{{2.5, 10}, {5.5, 7}, {8, 9.5}, {10, 7.5}, {13.5, 11}});
        if (state == HtmlImage::Failed) painter.drawLine(QPointF(2, 2), QPointF(14, 14));
    }
    painter.restore();
}

QString placeholderLabel(const HtmlImage &image) {
    switch (image.state) {
    case HtmlImage::Failed: return QStringLiteral("Image unavailable");
    case HtmlImage::Blocked: return QStringLiteral("Blocked domain");
    case HtmlImage::Insecure: return QStringLiteral("Not loaded: http");
    default: return image.alt.trimmed().isEmpty() ? QStringLiteral("Image") : image.alt.simplified();
    }
}

QFont placeholderFont() {
    QFont font(sansFamily());
    font.setPixelSize(12);
    return font;
}

void drawPlaceholder(QPainter &painter, const QRectF &area, const HtmlImage &image) {
    const QRectF box = area.adjusted(0.5, 0.5, -0.5, -0.5);
    if (box.width() <= 1 || box.height() <= 1) return;
    const bool small = area.width() < 48 || area.height() < 32;
    const bool chip = !small || area.height() >= chipHeight - 0.5;
    painter.save();
    painter.setRenderHint(QPainter::Antialiasing);
    if (image.state == HtmlImage::Loading) painter.setOpacity(0.55);
    const qreal radius = small ? 2 : 6;
    QPainterPath shape; shape.addRoundedRect(box, radius, radius);
    if (chip) painter.fillPath(shape, QBrush(stripes()));
    QPen border(QColor(0xc9, 0xce, 0xd6), 1);
    if (!small) border.setDashPattern({3, 3});
    painter.setPen(border); painter.setBrush(Qt::NoBrush);
    painter.drawPath(shape);
    if (chip && box.height() >= 18 && box.width() >= 30) {
        const QColor ink(0x8b, 0x91, 0x9a);
        const QFont font = placeholderFont();
        const QFontMetricsF metrics(font);
        const qreal room = box.width() - 24 - 22;
        const QString label = room > 12 ? metrics.elidedText(placeholderLabel(image), Qt::ElideRight, room) : QString();
        const qreal width = 14 + (label.isEmpty() ? 0 : 8 + metrics.horizontalAdvance(label));
        const qreal left = box.center().x() - width / 2, middle = box.center().y();
        placeholderIcon(painter, QRectF(left, middle - 7, 14, 14), image.state, ink);
        if (!label.isEmpty()) {
            painter.setFont(font); painter.setPen(ink);
            painter.drawText(QPointF(left + 22, middle + (metrics.ascent() - metrics.descent()) / 2), label);
        }
    }
    painter.restore();
}

Qt::PenStyle penStyle(litehtml::border_style style) {
    switch (style) {
    case litehtml::border_style_dotted: return Qt::DotLine;
    case litehtml::border_style_dashed: return Qt::DashLine;
    default: return Qt::SolidLine;
    }
}
bool visible(const litehtml::border &side) {
    return float(side.width) > 0 && side.style != litehtml::border_style_none && side.style != litehtml::border_style_hidden;
}
}

// litehtml's callbacks into the host. They run on the thread that calls
// HtmlDocument, during parse, render() and draw(). Every coordinate is a CSS
// pixel at 96 dpi; FmHtmlView scales the painter for the screen.
class HtmlContainer final : public litehtml::document_container {
public:
    qreal defaultFont = 16, width = 600;
    QHash<int, HtmlImage> images;
    QSet<int> imageElements;
    std::vector<std::pair<litehtml::position, litehtml::border_radiuses>> clips;

    QSizeF imageSize(int id) const {
        const auto found = images.constFind(id);
        if (found != images.cend() && found->state == HtmlImage::Ready && !found->image.isNull())
            return found->size.isValid() ? QSizeF(found->size) : QSizeF(found->image.size());
        // Undeclared sizes use a chip wide enough for the label.
        const HtmlImage empty;
        const QFontMetricsF metrics(placeholderFont());
        const auto label = placeholderLabel(found == images.cend() ? empty : *found);
        return QSizeF(std::clamp(metrics.horizontalAdvance(label) + 46, chipMinWidth, chipMaxWidth), chipHeight);
    }

    // The returned handle is an owned Font; litehtml keeps it for the
    // document's lifetime and releases it through delete_font().
    litehtml::uint_ptr create_font(const litehtml::font_description &description, const litehtml::document *,
                                   litehtml::font_metrics *fm) override {
        // litehtml bounds computed sizes, but a non-finite or huge size must
        // never reach qRound or the font engine.
        const float raw = float(description.size);
        const float size = std::isfinite(raw) ? std::clamp(raw, 1.0f, maxFontPixels) : maxFontPixels;
        QFont font(resolveFamily(description.family));
        font.setPixelSize(qRound(size));
        font.setWeight(QFont::Weight(std::clamp(description.weight, 100, 900)));
        font.setItalic(description.style == litehtml::font_style_italic);
        font.setUnderline(description.decoration_line & litehtml::text_decoration_line_underline);
        font.setOverline(description.decoration_line & litehtml::text_decoration_line_overline);
        font.setStrikeOut(description.decoration_line & litehtml::text_decoration_line_line_through);
        auto *handle = new Font{font, QFontMetricsF(font)};
        if (fm) {
            const auto &m = handle->metrics;
            fm->font_size = size;
            fm->ascent = float(m.ascent()); fm->descent = float(m.descent());
            fm->height = float(m.ascent() + m.descent());
            fm->x_height = float(m.xHeight());
            fm->ch_width = float(m.horizontalAdvance(QLatin1Char('0')));
            // Spaces are drawn so copied text keeps them.
            fm->draw_spaces = true;
            fm->sub_shift = size / 5;
            fm->super_shift = size / 3;
        }
        return reinterpret_cast<litehtml::uint_ptr>(handle);
    }
    void delete_font(litehtml::uint_ptr font) override { delete reinterpret_cast<Font *>(font); }
    litehtml::pixel_t text_width(const char *text, litehtml::uint_ptr font) override {
        return float(reinterpret_cast<Font *>(font)->metrics.horizontalAdvance(QString::fromUtf8(text)));
    }
    void draw_text(litehtml::uint_ptr hdc, const char *text, litehtml::uint_ptr handle, litehtml::web_color c,
                   const litehtml::position &pos) override {
        auto *canvas = reinterpret_cast<Canvas *>(hdc);
        const auto *font = reinterpret_cast<Font *>(handle);
        const auto value = QString::fromUtf8(text);
        if (canvas->record) {
            canvas->record->runs.append({rect(pos), value, font->font, 0});
            return;
        }
        auto &painter = *canvas->painter;
        painter.save(); applyClips(painter);
        painter.setFont(font->font); painter.setPen(color(c));
        // litehtml passes the text's line box; QPainter wants the baseline.
        painter.drawText(QPointF(float(pos.x), float(pos.y) + float(pos.height) - font->metrics.descent()), value);
        painter.restore();
    }
    // CSS defines 1pt as 1/72 in and 1px as 1/96 in, independent of the screen.
    litehtml::pixel_t pt_to_px(float pt) const override { return pt * 96.0f / 72.0f; }
    litehtml::pixel_t get_default_font_size() const override { return float(defaultFont); }
    const char *get_default_font_name() const override { return "sans-serif"; }
    void draw_list_marker(litehtml::uint_ptr hdc, const litehtml::list_marker &marker) override {
        auto *canvas = reinterpret_cast<Canvas *>(hdc);
        if (canvas->record) return;
        auto &painter = *canvas->painter;
        const QRectF box = rect(marker.pos);
        painter.save(); applyClips(painter);
        painter.setRenderHint(QPainter::Antialiasing);
        if (!marker.image.empty()) {
            const auto found = images.constFind(resourceId(marker.image));
            if (found != images.cend() && found->state == HtmlImage::Ready) painter.drawImage(box, found->image);
        } else if (marker.marker_type == litehtml::list_style_type_disc) {
            painter.setPen(Qt::NoPen); painter.setBrush(color(marker.color)); painter.drawEllipse(box);
        } else if (marker.marker_type == litehtml::list_style_type_circle) {
            painter.setPen(QPen(color(marker.color), 1)); painter.setBrush(Qt::NoBrush); painter.drawEllipse(box.adjusted(0.5, 0.5, -0.5, -0.5));
        } else if (marker.marker_type == litehtml::list_style_type_square) {
            painter.fillRect(box, color(marker.color));
        }
        painter.restore();
    }
    // Nothing loads from a URL here: the owner decodes `fm-res:N` images and
    // hands them over through HtmlDocument::setImage().
    void load_image(const char *, const char *, bool) override {}
    // Unloaded images take the placeholder chip's size, so the layout does
    // not collapse and jump when pixels arrive.
    void get_image_size(const char *src, const char *, litehtml::size &size) override {
        const auto value = imageSize(resourceId(src ? src : ""));
        size.width = float(value.width()); size.height = float(value.height());
    }
    void draw_image(litehtml::uint_ptr hdc, const litehtml::background_layer &layer, const std::string &url,
                    const std::string &) override {
        auto *canvas = reinterpret_cast<Canvas *>(hdc);
        const int id = resourceId(url);
        if (canvas->record || !id) return;
        const auto found = images.constFind(id);
        const bool element = imageElements.contains(id);
        const bool ready = found != images.cend() && found->state == HtmlImage::Ready && !found->image.isNull();
        // An unloaded background shows the element's own colour; only images get placeholders.
        if (!ready && !element) return;
        auto &painter = *canvas->painter;
        painter.save(); applyClips(painter); clipLayer(painter, layer);
        const QRectF origin = rect(layer.origin_box);
        if (!ready) drawPlaceholder(painter, origin, found == images.cend() ? HtmlImage{} : *found);
        else {
            painter.setRenderHint(QPainter::SmoothPixmapTransform);
            if (element || layer.repeat == litehtml::background_repeat_no_repeat) painter.drawImage(origin, found->image);
            else if (origin.width() >= 1 && origin.height() >= 1) {
                // Tiles are scaled once to their size, unless that size is
                // huge: the sender's background-size can be tens of thousands
                // of pixels, so the brush then scales the original as it fills.
                const QImage &image = found->image;
                const QSize tile = origin.size().toSize();
                QBrush brush;
                if (qint64(tile.width()) * tile.height() <= maxTilePixels) {
                    brush.setTextureImage(image.size() == tile ? image : image.scaled(tile, Qt::IgnoreAspectRatio, Qt::SmoothTransformation));
                    brush.setTransform(QTransform::fromTranslate(origin.x(), origin.y()));
                } else {
                    brush.setTextureImage(image);
                    brush.setTransform(QTransform::fromTranslate(origin.x(), origin.y())
                                           .scale(origin.width() / image.width(), origin.height() / image.height()));
                }
                QRectF area = rect(layer.clip_box);
                if (layer.repeat == litehtml::background_repeat_repeat_x) area = QRectF(area.left(), origin.top(), area.width(), origin.height());
                else if (layer.repeat == litehtml::background_repeat_repeat_y) area = QRectF(origin.left(), area.top(), origin.width(), area.height());
                painter.fillRect(area, brush);
            }
        }
        painter.restore();
    }
    void draw_solid_fill(litehtml::uint_ptr hdc, const litehtml::background_layer &layer, const litehtml::web_color &c) override {
        auto *canvas = reinterpret_cast<Canvas *>(hdc);
        if (canvas->record || c.alpha == 0) return;
        auto &painter = *canvas->painter;
        painter.save(); applyClips(painter);
        painter.setRenderHint(QPainter::Antialiasing);
        painter.setClipRect(rect(layer.clip_box), Qt::IntersectClip);
        painter.fillPath(rounded(rect(layer.border_box), layer.border_radius), color(c));
        painter.restore();
    }
    template <typename Gradient>
    void fillGradient(litehtml::uint_ptr hdc, const litehtml::background_layer &layer, Gradient &gradient,
                      const std::vector<litehtml::background_layer::color_point> &points, bool reversed = false) {
        auto *canvas = reinterpret_cast<Canvas *>(hdc);
        if (canvas->record) return;
        for (const auto &point : points)
            gradient.setColorAt(std::clamp(reversed ? 1.0 - point.offset : double(point.offset), 0.0, 1.0), color(point.color));
        auto &painter = *canvas->painter;
        painter.save(); applyClips(painter); clipLayer(painter, layer);
        painter.fillRect(rect(layer.clip_box), gradient);
        painter.restore();
    }
    void draw_linear_gradient(litehtml::uint_ptr hdc, const litehtml::background_layer &layer,
                              const litehtml::background_layer::linear_gradient &value) override {
        QLinearGradient gradient(QPointF(value.start.x, value.start.y), QPointF(value.end.x, value.end.y));
        fillGradient(hdc, layer, gradient, value.color_points);
    }
    void draw_radial_gradient(litehtml::uint_ptr hdc, const litehtml::background_layer &layer,
                              const litehtml::background_layer::radial_gradient &value) override {
        QRadialGradient gradient(QPointF(value.position.x, value.position.y), std::max(1.0f, value.radius.x));
        fillGradient(hdc, layer, gradient, value.color_points);
    }
    void draw_conic_gradient(litehtml::uint_ptr hdc, const litehtml::background_layer &layer,
                             const litehtml::background_layer::conic_gradient &value) override {
        // CSS turns clockwise from the top; Qt turns anticlockwise from the right.
        QConicalGradient gradient(QPointF(value.position.x, value.position.y), 90 - value.angle);
        fillGradient(hdc, layer, gradient, value.color_points, true);
    }
    void draw_borders(litehtml::uint_ptr hdc, const litehtml::borders &borders, const litehtml::position &pos, bool) override {
        auto *canvas = reinterpret_cast<Canvas *>(hdc);
        if (canvas->record || !borders.is_visible()) return;
        auto &painter = *canvas->painter;
        const QRectF box = rect(pos);
        painter.save(); applyClips(painter);
        painter.setRenderHint(QPainter::Antialiasing);
        const auto &t = borders.top, &r = borders.right, &b = borders.bottom, &l = borders.left;
        const bool uniform = float(t.width) == float(r.width) && float(t.width) == float(b.width) && float(t.width) == float(l.width)
            && t.style == r.style && t.style == b.style && t.style == l.style
            && t.color == r.color && t.color == b.color && t.color == l.color;
        if (uniform && visible(t)) {
            const qreal w = float(t.width);
            QPen pen(color(t.color), w, penStyle(t.style)); pen.setJoinStyle(Qt::MiterJoin);
            painter.setPen(pen); painter.setBrush(Qt::NoBrush);
            auto radius = borders.radius;
            for (auto *value : {&radius.top_left_x, &radius.top_left_y, &radius.top_right_x, &radius.top_right_y,
                                &radius.bottom_right_x, &radius.bottom_right_y, &radius.bottom_left_x, &radius.bottom_left_y})
                *value = std::max(0.0f, float(*value) - float(w / 2));
            painter.drawPath(rounded(box.adjusted(w / 2, w / 2, -w / 2, -w / 2), radius));
        } else {
            // Mixed sides are mitred trapezoids; corner radii are not drawn.
            const qreal tw = visible(t) ? float(t.width) : 0, rw = visible(r) ? float(r.width) : 0;
            const qreal bw = visible(b) ? float(b.width) : 0, lw = visible(l) ? float(l.width) : 0;
            painter.setPen(Qt::NoPen);
            const auto side = [&](const litehtml::border &border, qreal width, const QPolygonF &shape) {
                if (width <= 0) return;
                painter.setBrush(color(border.color)); painter.drawPolygon(shape);
            };
            side(t, tw, {box.topLeft(), box.topRight(), {box.right() - rw, box.top() + tw}, {box.left() + lw, box.top() + tw}});
            side(r, rw, {box.topRight(), box.bottomRight(), {box.right() - rw, box.bottom() - bw}, {box.right() - rw, box.top() + tw}});
            side(b, bw, {box.bottomLeft(), box.bottomRight(), {box.right() - rw, box.bottom() - bw}, {box.left() + lw, box.bottom() - bw}});
            side(l, lw, {box.topLeft(), box.bottomLeft(), {box.left() + lw, box.bottom() - bw}, {box.left() + lw, box.top() + tw}});
        }
        painter.restore();
    }
    // Titles, base URLs, <link> elements and pointer interaction are unused:
    // FmHtmlView handles hover, clicks and cursors from the snapshot's link boxes.
    void set_caption(const char *) override {}
    void set_base_url(const char *) override {}
    void link(const std::shared_ptr<litehtml::document> &, const litehtml::element::ptr &) override {}
    void on_anchor_click(const char *, const litehtml::element::ptr &) override {}
    void on_mouse_event(const litehtml::element::ptr &, litehtml::mouse_event) override {}
    void set_cursor(const char *) override {}
    void transform_text(std::string &text, litehtml::text_transform transform) override {
        auto value = QString::fromStdString(text);
        if (transform == litehtml::text_transform_uppercase) value = value.toUpper();
        else if (transform == litehtml::text_transform_lowercase) value = value.toLower();
        else if (transform == litehtml::text_transform_capitalize && !value.isEmpty()) value[0] = value[0].toUpper();
        text = value.toStdString();
    }
    // litehtml ends lines only at spaces. Like browsers, also allow a break
    // after "/", "-" and similar inside long runs, at zero-width spaces, and
    // in 30-character pieces of any run over 60, so logs, URLs and hashes wrap
    // instead of widening the message. Pieces are words with no space between.
    void split_text(const char *text, const std::function<void(const char *)> &on_word,
                    const std::function<void(const char *)> &on_space) override {
        const std::u32string input = static_cast<const char32_t *>(litehtml::utf8_to_utf32(text));
        const auto space = [](char32_t c) { return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f'; };
        std::u32string word;
        bool longRun = false;
        const auto flush = [&] { if (!word.empty()) { on_word(litehtml::utf32_to_utf8(word)); word.clear(); } };
        for (size_t i = 0; i < input.size(); ++i) {
            const char32_t c = input[i];
            if (space(c)) {
                flush();
                on_space(litehtml::utf32_to_utf8(std::u32string(1, c)));
                continue;
            }
            if (i == 0 || space(input[i - 1])) {
                size_t end = i;
                while (end < input.size() && !space(input[end])) ++end;
                longRun = end - i > 60;
            }
            if (c == 0x200B) { flush(); continue; }
            if (c >= 0x4E00 && c <= 0x9FCC) { flush(); word += c; flush(); continue; }
            word += c;
            const bool more = i + 1 < input.size() && !space(input[i + 1]);
            const bool punctuation = c == '/' || c == '-' || c == '?' || c == '&' || c == '=' || c == '|' || c == '\\';
            if (more && ((punctuation && word.size() > 1) || (longRun && word.size() >= 30))) flush();
        }
        flush();
    }
    // Sanitised mail has no stylesheet links; nothing is ever imported.
    void import_css(std::string &text, const std::string &, std::string &) override { text.clear(); }
    // Overflow clips nest as a stack. Each draw callback wraps its painting in
    // save()/restore(), so the whole stack is re-applied per call.
    void set_clip(const litehtml::position &pos, const litehtml::border_radiuses &radius) override { clips.emplace_back(pos, radius); }
    void del_clip() override { if (!clips.empty()) clips.pop_back(); }
    // The message scrolls as one tall item, so there is no real viewport
    // height; 1000 px is a fixed nominal value for vh units and media queries.
    void get_viewport(litehtml::position &viewport) const override { viewport = litehtml::position(0, 0, float(width), 1000); }
    litehtml::element::ptr create_element(const char *, const litehtml::string_map &, const std::shared_ptr<litehtml::document> &) override { return nullptr; }
    void get_media_features(litehtml::media_features &media) const override {
        media.type = litehtml::media_type_screen;
        media.width = media.device_width = float(width);
        media.height = media.device_height = 1000;
        media.color = 8; media.monochrome = 0; media.color_index = 256; media.resolution = 96;
    }
    void get_language(std::string &language, std::string &culture) const override { language = "en"; culture.clear(); }

    void applyClips(QPainter &painter) const {
        for (const auto &[pos, radius] : clips) painter.setClipPath(rounded(rect(pos), radius), Qt::IntersectClip);
    }
    static void clipLayer(QPainter &painter, const litehtml::background_layer &layer) {
        painter.setClipPath(rounded(rect(layer.border_box), layer.border_radius), Qt::IntersectClip);
        painter.setClipRect(rect(layer.clip_box), Qt::IntersectClip);
    }
};

namespace {
void walk(const litehtml::element::ptr &element, const std::function<void(const litehtml::element::ptr &)> &visit) {
    if (!element) return;
    visit(element);
    for (const auto &child : element->children()) walk(child, visit);
}
bool isTag(const litehtml::element::ptr &element, const char *name) {
    const char *tag = element->get_tagName();
    return tag && std::strcmp(tag, name) == 0;
}
}

HtmlDocument::HtmlDocument(const QByteArray &html, qreal defaultFontPixels, const QSet<int> &blockedLinks,
                           const QHash<int, HtmlImage> &images)
    : m_container(std::make_unique<HtmlContainer>()) {
    m_container->defaultFont = defaultFontPixels;
    m_container->images = images;
    for (const auto &image : images) liveBytes += image.image.sizeInBytes();
    // Blocked links read as disabled, as in the design. One marked attribute
    // and one rule, however many links a message has: a rule per link would
    // make litehtml walk every element's ancestors once for each of them.
    QByteArray marked;
    std::string user;
    if (!blockedLinks.isEmpty()) {
        static const QByteArray prefix = "href=\"fm-link:";
        marked.reserve(html.size() + blockedLinks.size() * 16);
        qsizetype from = 0;
        for (qsizetype at = html.indexOf(prefix); at >= 0; at = html.indexOf(prefix, from)) {
            const auto digits = at + prefix.size(), close = html.indexOf('"', digits);
            if (close < 0) break;
            bool numeric = false;
            const int id = html.mid(digits, close - digits).toInt(&numeric);
            marked.append(html.constData() + from, close + 1 - from);
            if (numeric && blockedLinks.contains(id)) marked.append(" data-fm-blocked");
            from = close + 1;
        }
        marked.append(html.constData() + from, html.size() - from);
        user = "a[data-fm-blocked],a[data-fm-blocked] *{color:#9aa0a6 !important;text-decoration:line-through !important}";
    }
    // Preformatted text wraps rather than widening the whole message; the
    // sender's own white-space rules still win.
    static const std::string master = std::string(litehtml::master_css) + "pre,xmp,plaintext,listing{white-space:pre-wrap}";
    m_document = litehtml::document::createFromString((blockedLinks.isEmpty() ? html : marked).toStdString(), m_container.get(), master, user);
    if (!m_document) return;
    walk(m_document->root(), [this](const litehtml::element::ptr &element) {
        if (isTag(element, "img"))
            if (const int id = prefixedId(element->get_attr("src"), "fm-res:")) m_container->imageElements.insert(id);
    });
}

HtmlDocument::~HtmlDocument() {
    for (const auto &image : m_container->images) liveBytes -= image.image.sizeInBytes();
}

bool HtmlDocument::valid() const { return m_document && m_document->root(); }

bool HtmlDocument::setImage(int id, const HtmlImage &image) {
    // Only an <img> element's box follows the image size; background images
    // never size a box. Compare sizes to report whether layout is now stale.
    const auto before = m_container->imageSize(id);
    if (const auto old = m_container->images.constFind(id); old != m_container->images.cend()) liveBytes -= old->image.sizeInBytes();
    liveBytes += image.image.sizeInBytes();
    m_container->images.insert(id, image);
    return m_container->imageElements.contains(id) && m_container->imageSize(id) != before;
}

QSizeF HtmlDocument::layout(qreal width, std::chrono::steady_clock::time_point deadline) {
    if (!valid()) return {};
    m_container->width = width;
    // Width-dependent media queries must see the new width before render().
    m_document->media_changed();
    m_document->set_render_deadline(deadline);
    m_document->render(float(width));
    // A layout cut short leaves litehtml's boxes half placed; drop it.
    if (m_document->render_timed_out()) { m_document.reset(); return {}; }
    return clampedSize(QSizeF(std::max<qreal>(width, float(m_document->width())), float(m_document->height())));
}

HtmlSnapshot HtmlDocument::snapshot() {
    HtmlSnapshot result;
    if (!valid()) return result;
    result.size = clampedSize(QSizeF(float(m_document->width()), float(m_document->height())));
    // A record-mode pass over the whole document collects the text runs in
    // paint order; the clip stack is reset around it in case a previous pass
    // ended unbalanced.
    Canvas canvas{nullptr, &result};
    const litehtml::position all(0, 0, m_document->width(), m_document->height());
    m_container->clips.clear();
    m_document->draw(reinterpret_cast<litehtml::uint_ptr>(&canvas), 0, 0, &all);
    m_container->clips.clear();
    // Copied text breaks lines where runs move down a line.
    const HtmlSnapshot::Run *previous = nullptr;
    for (auto &run : result.runs) {
        if (previous && run.rect.top() >= previous->rect.bottom() - previous->rect.height() / 2) result.text += '\n';
        run.offset = int(result.text.size());
        result.text += run.text;
        previous = &run;
    }
    walk(m_document->root(), [&result](const litehtml::element::ptr &element) {
        if (!isTag(element, "a")) return;
        const int id = prefixedId(element->get_attr("href"), "fm-link:");
        const auto render = id ? element->get_render_item() : nullptr;
        if (!render) return;
        render->get_rendering_boxes([&](const litehtml::position &box) {
            if (float(box.width) > 0 && float(box.height) > 0) result.links.append({id, rect(box)});
        });
    });
    return result;
}

void HtmlDocument::paint(QPainter &painter, const QRectF &clip) {
    if (!valid()) return;
    Canvas canvas{&painter, nullptr};
    const litehtml::position area(float(clip.x()), float(clip.y()), float(clip.width()), float(clip.height()));
    m_container->clips.clear();
    m_document->draw(reinterpret_cast<litehtml::uint_ptr>(&canvas), 0, 0, &area);
    m_container->clips.clear();
}

qint64 HtmlDocument::liveImageBytes() { return liveBytes; }
