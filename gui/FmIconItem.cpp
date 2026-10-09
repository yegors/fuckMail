// FmIconItem: the app's line icons as QPainter paths, rendered once per window,
// name, colour and size into a shared texture cache.
#include "FmIconItem.h"
#include "FmHtmlView.h"
#include <QHash>
#include <QPainter>
#include <QPainterPath>
#include <QQuickWindow>
#include <QSGImageNode>
#include <QSGTexture>
#include <QtMath>
#include <QtQml/qqml.h>
#include <initializer_list>

namespace {
// The former Canvas 2D drawing calls, on the same 16×16 grid.
struct Sketch {
    QPainterPath path;
    void moveTo(qreal x, qreal y) { path.moveTo(x, y); }
    void lineTo(qreal x, qreal y) { path.lineTo(x, y); }
    void line(std::initializer_list<qreal> points) {
        auto p = points.begin();
        path.moveTo(p[0], p[1]);
        for (size_t i = 2; i + 1 < points.size(); i += 2) path.lineTo(p[i], p[i + 1]);
    }
    // Canvas arc(): clockwise on screen from start to end, in radians. Qt's
    // path angles are degrees counter-clockwise, so both signs flip.
    void arc(qreal x, qreal y, qreal r, qreal start, qreal end) {
        const qreal from = -qRadiansToDegrees(start), to = -qRadiansToDegrees(end);
        qreal span = to - from;
        if (from - to >= 360) span = -360;
        else if (span > 0) span -= 360;
        path.arcTo(QRectF(x - r, y - r, 2 * r, 2 * r), from, span);
    }
    void circle(qreal x, qreal y, qreal r) { moveTo(x + r, y); arc(x, y, r, 0, 2 * M_PI); }
    void quadTo(qreal cx, qreal cy, qreal x, qreal y) { path.quadTo(cx, cy, x, y); }
    void cubicTo(qreal ax, qreal ay, qreal bx, qreal by, qreal x, qreal y) { path.cubicTo(ax, ay, bx, by, x, y); }
    void roundedRect(qreal x, qreal y, qreal w, qreal h, qreal r) { path.addRoundedRect(QRectF(x, y, w, h), r, r); }
    // SVG elliptical arc with equal radii and no rotation, from the current point.
    // Canvas parsed SVG paths with Qt's SVG arc code, which splits an arc into
    // quarter-turn cubic segments; the same segments give the same pixels.
    void svgArc(qreal r, bool large, bool sweep, qreal x, qreal y) {
        const auto from = path.currentPosition();
        const qreal dx = (from.x() - x) / 2, dy = (from.y() - y) / 2;
        const qreal check = (dx * dx + dy * dy) / (r * r);
        if (check > 1) r *= qSqrt(check);
        const qreal x0 = from.x() / r, y0 = from.y() / r, x1 = x / r, y1 = y / r;
        const qreal d = (x1 - x0) * (x1 - x0) + (y1 - y0) * (y1 - y0);
        if (!d) return;
        qreal factor = qSqrt(qMax<qreal>(0, 1 / d - 0.25));
        if (sweep == large) factor = -factor;
        const qreal xc = 0.5 * (x0 + x1) - factor * (y1 - y0), yc = 0.5 * (y0 + y1) + factor * (x1 - x0);
        const qreal th0 = qAtan2(y0 - yc, x0 - xc);
        qreal arc = qAtan2(y1 - yc, x1 - xc) - th0;
        if (arc < 0 && sweep) arc += 2 * M_PI;
        else if (arc > 0 && !sweep) arc -= 2 * M_PI;
        const int segments = qCeil(qAbs(arc / (M_PI * 0.5 + 0.001)));
        for (int i = 0; i < segments; ++i) {
            const qreal a = th0 + i * arc / segments, b = th0 + (i + 1) * arc / segments, half = 0.5 * (b - a);
            const qreal t = (8.0 / 3.0) * qSin(half * 0.5) * qSin(half * 0.5) / qSin(half);
            path.cubicTo(r * (xc + qCos(a) - t * qSin(a)), r * (yc + qSin(a) + t * qCos(a)),
                         r * (xc + qCos(b) + t * qSin(b)), r * (yc + qSin(b) - t * qCos(b)),
                         r * (xc + qCos(b)), r * (yc + qSin(b)));
        }
    }
};

// Returns false for an icon that is filled rather than stroked.
bool draw(const QString &name, Sketch &c) {
    const auto n = name.toLatin1();
    if (n == "pause") { c.line({5.5,3.5,5.5,12.5}); c.line({10.5,3.5,10.5,12.5}); }
    else if (n == "play") c.line({5,3.5,12,8,5,12.5,5,3.5});
    else if (n == "summary") { c.line({8,1.5,8,4.7}); c.line({8,11.3,8,14.5}); c.line({2.4,4.75,5.2,6.35}); c.line({10.8,9.65,13.6,11.25}); c.line({2.4,11.25,5.2,9.65}); c.line({10.8,6.35,13.6,4.75}); c.circle(8,8,1.6); }
    else if (n == "volume") { c.line({2,6,5,6,8,3.5,8,12.5,5,10,2,10,2,6}); c.moveTo(10,5); c.arc(8,8,4,-0.75,0.75); }
    else if (n == "mute") { c.line({2,6,5,6,8,3.5,8,12.5,5,10,2,10,2,6}); c.line({10,6,14,10}); c.line({14,6,10,10}); }
    else if (n == "mail") { c.roundedRect(2,3.5,12,9,1.3); c.line({2.5,4.5,8,8.7,13.5,4.5}); }
    else if (n == "send") { c.line({14,2,9.5,14,7,9,2,6.5,14,2}); c.line({7,9,14,2}); }
    else if (n == "calendar") { c.roundedRect(2.5,3.5,11,10,1); c.line({2.5,7,13.5,7}); c.line({5.5,2,5.5,5}); c.line({10.5,2,10.5,5}); }
    else if (n == "search") { c.circle(7,7,4.5); c.line({10.5,10.5,14,14}); }
    else if (n == "close") { c.line({4,4,12,12}); c.line({12,4,4,12}); }
    else if (n == "previous") c.line({10,3.5,5.5,8,10,12.5});
    else if (n == "next") c.line({6,3.5,10.5,8,6,12.5});
    else if (n == "chevronDown" || n == "down") c.line({4,6,8,10,12,6});
    else if (n == "chevronUp" || n == "up") c.line({4,10,8,6,12,10});
    else if (n == "arrowUp") { c.line({8,13.5,8,2.5}); c.line({3.5,7,8,2.5,12.5,7}); }
    else if (n == "pin") { c.moveTo(8,14); c.quadTo(3.5,9.8,3.5,6.5); c.arc(8,6.5,4.5,M_PI,0); c.quadTo(12.5,9.8,8,14); c.circle(8,6.5,1.5); }
    else if (n == "video") { c.roundedRect(1.5,4,9,8,1.5); c.line({10.5,7,14.5,4.5,14.5,11.5,10.5,9}); }
    else if (n == "people") { c.circle(6,5.5,2.25); c.moveTo(1.5,13); c.arc(6,13,4.5,M_PI,0); c.moveTo(10.5,3.5); c.arc(10.5,5.6,2.1,-M_PI/2,M_PI/2); c.moveTo(12,9.2); c.quadTo(14.3,10.3,14.5,13); }
    else if (n == "arrowDown") { c.line({8,2.5,8,13.5}); c.line({3.5,9,8,13.5,12.5,9}); }
    else if (n == "arrowRight") { c.line({3,8,12.5,8}); c.line({9,4.5,12.5,8,9,11.5}); }
    else if (n == "undo") { c.line({6,2.5,2.5,6,6,9.5}); c.moveTo(3,6); c.cubicTo(14,3,15,13,8,13); }
    else if (n == "redo") { c.line({10,2.5,13.5,6,10,9.5}); c.moveTo(13,6); c.cubicTo(2,3,1,13,8,13); }
    else if (n == "trash") { c.line({2.5,4,13.5,4}); c.line({6,2,10,2}); c.line({4,4,4.7,14,11.3,14,12,4}); c.line({6.5,6.5,6.8,11.5}); c.line({9.5,6.5,9.2,11.5}); }
    else if (n == "check") c.line({3.5,8,6.5,11,12.5,4.5});
    // M8 2 13 4v4c0 3-2.2 5.2-5 6-2.8-.8-5-3-5-6V4zM6 8l1.5 1.5L10.5 6.5
    else if (n == "shield") { c.moveTo(8,2); c.lineTo(13,4); c.lineTo(13,8); c.cubicTo(13,11,10.8,13.2,8,14); c.cubicTo(5.2,13.2,3,11,3,8); c.lineTo(3,4); c.path.closeSubpath(); c.line({6,8,7.5,9.5,10.5,6.5}); }
    // M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8.5h5.8l.6-8.5
    else if (n == "bin") { c.line({3,4.5,13,4.5}); c.line({6.5,4.5,6.5,3,9.5,3,9.5,4.5}); c.line({4.5,4.5,5.1,13,10.9,13,11.5,4.5}); }
    // M2.5 9.5h3l1 1.5h3l1-1.5h3M2.5 9.5 4 4h8l1.5 5.5V13h-11z
    else if (n == "inbox") { c.line({2.5,9.5,5.5,9.5,6.5,11,9.5,11,10.5,9.5,13.5,9.5}); c.line({2.5,9.5,4,4,12,4,13.5,9.5,13.5,13,2.5,13,2.5,9.5}); }
    // M3 7.5h7a3 3 0 0 1 0 6H7M5.5 5 3 7.5 5.5 10
    else if (n == "restore") { c.moveTo(3,7.5); c.lineTo(10,7.5); c.svgArc(3,false,true,10,13.5); c.lineTo(7,13.5); c.line({5.5,5,3,7.5,5.5,10}); }
    // M4.5 12.5h7a3 3 0 0 0 .4-6 4 4 0 0 0-7.7 1A2.5 2.5 0 0 0 4.5 12.5z
    else if (n == "cloud") { c.moveTo(4.5,12.5); c.lineTo(11.5,12.5); c.svgArc(3,false,false,11.9,6.5); c.svgArc(4,false,false,4.2,7.5); c.svgArc(2.5,false,false,4.5,12.5); c.path.closeSubpath(); }
    else if (n == "external") { c.line({9,3,13,3,13,7}); c.line({13,3,7.5,8.5}); c.line({11.5,9.5,11.5,13,2.5,13,2.5,4.5,6,4.5}); }
    else if (n == "plus") { c.line({8,3,8,13}); c.line({3,8,13,8}); }
    else if (n == "minus") c.line({3.5,8,12.5,8});
    else if (n == "bulletList") { for (const qreal y : {4.0, 8.0, 12.0}) { c.circle(3,y,0.9); c.line({6,y,13.5,y}); } }
    else if (n == "numberedList") {
        c.line({2,2.8,3,2.2,3,5.6});
        c.moveTo(1.9,7.4); c.cubicTo(2.2,6.2,4.2,6.4,3.9,7.7); c.lineTo(1.9,9.8); c.lineTo(4.1,9.8);
        c.moveTo(2,11.4); c.cubicTo(3,10.6,4.3,11.2,3.4,12.3); c.cubicTo(4.5,12.6,3.9,14.4,2,13.7);
        for (const qreal y : {4.0, 8.0, 12.0}) c.line({6.5,y,13.5,y});
    }
    else if (n == "combined") { c.roundedRect(2.5,2.5,6.5,6.5,1); c.roundedRect(7,7,6.5,6.5,1); }
    else if (n == "filter") { c.line({2.5,4,13.5,4}); c.line({5,8,11,8}); c.line({6.5,12,9.5,12}); }
    else if (n == "more") { c.circle(3,8,0.7); c.circle(8,8,0.7); c.circle(13,8,0.7); }
    else if (n == "accounts") { c.roundedRect(2,5,12,9,1.5); c.line({4.5,5,4.5,2.5,11.5,2.5,11.5,5}); }
    else if (n == "person") { c.circle(8,4.5,2.3); c.moveTo(2.8,14); c.cubicTo(2.8,7.8,13.2,7.8,13.2,14); }
    else if (n == "bot") { c.roundedRect(3,6,10,7,1.5); c.line({8,6,8,3.5}); c.line({6.5,9.5,6.51,9.5}); c.line({9.5,9.5,9.51,9.5}); c.line({1.5,9,1.5,11}); c.line({14.5,9,14.5,11}); }
    else if (n == "unknown") { c.circle(8,8,6); c.moveTo(6.1,5.5); c.cubicTo(6.1,3,10.8,3,10.2,5.8); c.cubicTo(10,6.8,8,7,8,8.8); c.line({8,11,8,11.2}); }
    else if (n == "wind") {
        c.line({2,5,8.5,5}); c.cubicTo(11,5,11,1.5,8.5,1.5); c.cubicTo(7.5,1.5,6.75,2.3,6.75,3.25);
        c.line({2,8.5,11.5,8.5}); c.cubicTo(14,8.5,14,12,11.5,12); c.cubicTo(10.5,12,9.75,11.25,9.75,10.25);
        c.line({2,12,7.5,12});
    }
    else if (n == "sliders") { c.line({2.5,5.5,4,5.5}); c.line({7.5,5.5,13.5,5.5}); c.line({2.5,10.5,8.5,10.5}); c.line({12,10.5,13.5,10.5}); c.circle(5.75,5.5,1.75); c.circle(10.25,10.5,1.75); }
    else if (n == "general") { c.line({3,4.5,13,4.5}); c.line({3,8,13,8}); c.line({3,11.5,9,11.5}); c.circle(11.5,11.5,1.5); }
    else if (n == "chip") { c.roundedRect(3.5,3.5,9,9,1.5); c.line({6,1.5,6,3.5}); c.line({10,1.5,10,3.5}); c.line({6,12.5,6,14.5}); c.line({10,12.5,10,14.5}); c.line({1.5,6,3.5,6}); c.line({1.5,10,3.5,10}); c.line({12.5,6,14.5,6}); c.line({12.5,10,14.5,10}); }
    else if (n == "keyboard") { c.roundedRect(1.5,4,13,8,1.5); c.line({4,7,5,7}); c.line({7.5,7,8.5,7}); c.line({11,7,12,7}); c.line({5,9.5,11,9.5}); }
    else if (n == "clock") { c.circle(8,8,6); c.line({8,4,8,8,10.7,9.6}); }
    else if (n == "info") { c.circle(8,8,6); c.line({8,7.5,8,11}); c.line({8,4.8,8,5}); }
    else if (n == "warning") { c.moveTo(8,2.5); c.lineTo(14,13); c.lineTo(2,13); c.path.closeSubpath(); c.line({8,6.5,8,9.5}); c.line({8,11.3,8,11.5}); }
    else if (n == "review") { c.circle(8,8,5.5); c.line({8,5.2,8,8.5}); c.line({8,10.6,8,10.9}); }
    else if (n == "checkCircle") { c.circle(8,8,6); c.line({5.5,8.2,7.3,10,10.7,6.2}); }
    else if (n == "reply") { c.line({6.5,3.5,2.5,7.5,6.5,11.5}); c.moveTo(3,7.5); c.cubicTo(10,6,13.5,9.5,13.5,13); }
    else if (n == "replyAll") { c.line({5,3.5,1,7.5,5,11.5}); c.line({8,3.5,4,7.5,8,11.5}); c.moveTo(5,7.5); c.cubicTo(11,6,14,9.5,14,13); }
    else if (n == "forward") { c.line({9.5,3.5,13.5,7.5,9.5,11.5}); c.moveTo(13,7.5); c.cubicTo(6,6,2.5,9.5,2.5,13); }
    else if (n == "edit") { c.line({10,3,13,6,6,13,2.5,13.5,3,10,10,3}); c.line({9,4,12,7}); }
    else if (n == "refresh") { c.moveTo(13,7); c.arc(8,8,5,-0.2,M_PI*0.86); c.line({3,13,3,10,6,10}); c.moveTo(3,9); c.arc(8,8,5,M_PI-0.2,M_PI*1.86); c.line({13,3,13,6,10,6}); }
    else if (n == "settings") {
        c.circle(8,8,2.1);
        for (int i = 0; i < 8; ++i) {
            const qreal a = M_PI * i / 4, b = a + 0.14, d = a + 0.64;
            c.line({8+5*qCos(a),8+5*qSin(a),8+6.4*qCos(b),8+6.4*qSin(b),8+6.4*qCos(d),8+6.4*qSin(d),8+5*qCos(a+M_PI/4),8+5*qSin(a+M_PI/4)});
        }
    }
    // A starred contact: the outline to star one, filled once starred.
    else if (n == "contactStar" || n == "contactStarFilled") {
        c.line({8,2.2,9.8,6,13.9,6.5,10.9,9.4,11.7,13.5,8,11.5,4.3,13.5,5.1,9.4,2.1,6.5,6.2,6,8,2.2});
        if (n == "contactStarFilled") return false;
    }
    else if (n == "star") {
        for (int j = 0; j <= 10; ++j) {
            const qreal a = -M_PI/2 + j*M_PI/5, r = j % 2 ? 2.8 : 6;
            if (j == 0) c.moveTo(8+r*qCos(a),8+r*qSin(a)); else c.lineTo(8+r*qCos(a),8+r*qSin(a));
        }
    }
    else if (n == "file") { c.line({3,1,9,1,13,5,13,15,3,15,3,1}); c.line({9,1,9,5,13,5}); }
    else if (n == "image") { c.line({2.5,3.5,13.5,3.5,13.5,12.5,2.5,12.5,2.5,3.5}); c.line({2.5,10,5.5,7,8,9.5,10,7.5,13.5,11}); }
    else if (n == "imageOff") { c.line({2.5,3.5,13.5,3.5,13.5,12.5,2.5,12.5,2.5,3.5}); c.line({2.5,10,5.5,7,8,9.5,10,7.5,13.5,11}); c.line({2,2,14,14}); }
    else if (n == "globe") { c.circle(8,8,6); c.line({2,8,14,8}); c.moveTo(8,2); c.cubicTo(10,4.2,10,11.8,8,14); c.moveTo(8,2); c.cubicTo(6,4.2,6,11.8,8,14); }
    // Pipeline rows: mail arriving, and an account's labels.
    else if (n == "fetch") { c.line({8,2.5,8,10.5}); c.line({4.5,7,8,10.5,11.5,7}); c.line({3,13,13,13}); }
    else if (n == "tag") { c.line({2.5,2.5,7.7,2.5,13.5,8.3,8.3,13.5,2.5,7.7,2.5,2.5}); c.circle(5.4,5.4,0.9); }
    else if (n == "download") { c.line({8,2,8,10}); c.line({4.5,6.5,8,10,11.5,6.5}); c.line({2.5,11,2.5,14,13.5,14,13.5,11}); }
    else if (n == "google") {
        c.moveTo(13.6,8.2); c.cubicTo(13.6,7.8,13.6,7.5,13.5,7.2); c.lineTo(8,7.2); c.lineTo(8,9.2); c.lineTo(11.2,9.2);
        c.svgArc(2.8,false,true,10,11); c.lineTo(10,12.5); c.lineTo(11.9,12.5); c.cubicTo(13,11.5,13.6,10,13.6,8.2); c.path.closeSubpath();
        c.moveTo(8,13.8); c.cubicTo(9.6,13.8,10.9,13.3,11.9,12.4); c.lineTo(10,10.9); c.cubicTo(9.5,11.3,8.8,11.5,8,11.5);
        c.cubicTo(6.5,11.5,5.2,10.5,4.7,9.1); c.lineTo(2.8,9.1); c.lineTo(2.8,10.6); c.svgArc(5.8,false,false,8,13.8); c.path.closeSubpath();
        c.moveTo(4.7,9.1); c.svgArc(3.5,false,true,4.7,6.9); c.lineTo(4.7,5.4); c.lineTo(2.8,5.4); c.svgArc(5.8,false,false,2.8,10.6); c.path.closeSubpath();
        c.moveTo(8,4.5); c.cubicTo(8.9,4.5,9.6,4.8,10.2,5.4); c.lineTo(11.9,3.7); c.svgArc(5.8,false,false,2.8,5.4); c.lineTo(4.7,6.9);
        c.cubicTo(5.2,5.5,6.5,4.5,8,4.5); c.path.closeSubpath();
        return false;
    }
    else if (n == "thread") { c.moveTo(2.5,4.5); c.svgArc(1.5,false,true,4,3); c.lineTo(12,3); c.svgArc(1.5,false,true,13.5,4.5); c.lineTo(13.5,9.5); c.svgArc(1.5,false,true,12,11); c.lineTo(7,11); c.lineTo(4,13.5); c.lineTo(4,11); c.svgArc(1.5,false,true,2.5,9.5); c.path.closeSubpath(); }
    else if (n == "attachment") { c.moveTo(5,10); c.lineTo(10,5); c.cubicTo(13,2,16,5,13,8); c.lineTo(7,14); c.cubicTo(3,18,-1,12,3,8); c.lineTo(9,2); }
    return true;
}

struct IconKey {
    QString name;
    QRgb color;
    QSize pixels;
    QSizeF scale;
    bool operator==(const IconKey &other) const = default;
};

size_t qHash(const IconKey &key, size_t seed = 0) {
    return qHashMulti(seed, key.name, key.color, key.pixels.width(), key.pixels.height(), key.scale.width(), key.scale.height());
}
}

// One texture per icon image and window, created and released on the render thread.
// The cache object itself is a child of its window, created from itemChange()
// on the GUI thread; only texture() and clear() run on the render thread.
class FmIconCache final : public QObject {
public:
    static FmIconCache *of(QQuickWindow *window) {
        auto &cache = caches()[window];
        if (!cache) cache = new FmIconCache(window);
        return cache;
    }
    // Called from updatePaintNode(), so the render thread creates the texture
    // while the GUI thread is blocked in the sync step. Entries are never
    // evicted individually; keys are bounded by the icon names, theme colours
    // and sizes the UI actually uses.
    QSGTexture *texture(QQuickWindow *window, const IconKey &key) {
        auto &texture = m_textures[key];
        if (!texture) texture = window->createTextureFromImage(
            FmIconItem::render(key.name, QColor::fromRgba(key.color), key.pixels, key.scale));
        return texture;
    }
    ~FmIconCache() override { caches().remove(m_window); clear(); }
private:
    explicit FmIconCache(QQuickWindow *window) : QObject(window), m_window(window) {
        // A direct connection runs clear() on the render thread while the
        // graphics context still exists, so textures die where they were made.
        // The next updatePaintNode() recreates whatever is still shown.
        connect(window, &QQuickWindow::sceneGraphInvalidated, this, &FmIconCache::clear, Qt::DirectConnection);
    }
    static QHash<QQuickWindow *, FmIconCache *> &caches() { static QHash<QQuickWindow *, FmIconCache *> all; return all; }
    void clear() { qDeleteAll(m_textures); m_textures.clear(); }
    QQuickWindow *m_window;
    QHash<IconKey, QSGTexture *> m_textures;
};

FmIconItem::FmIconItem(QQuickItem *parent) : QQuickItem(parent) {
    setFlag(ItemHasContents);
    setImplicitSize(m_size, m_size);
    setSize(QSizeF(m_size, m_size));
}

void FmIconItem::setName(const QString &name) {
    if (m_name == name) return;
    m_name = name; emit nameChanged(); update();
}

void FmIconItem::setColor(const QColor &color) {
    if (m_color == color) return;
    m_color = color; emit colorChanged(); update();
}

void FmIconItem::setIconSize(int size) {
    if (m_size == size) return;
    m_size = size;
    setImplicitSize(size, size);
    setSize(QSizeF(size, size));
    emit sizeChanged();
}

void FmIconItem::geometryChange(const QRectF &newGeometry, const QRectF &oldGeometry) {
    QQuickItem::geometryChange(newGeometry, oldGeometry);
    if (newGeometry.size() != oldGeometry.size()) update();
}

void FmIconItem::itemChange(ItemChange change, const ItemChangeData &data) {
    QQuickItem::itemChange(change, data);
    // Look the cache up here, on the GUI thread, so updatePaintNode() never
    // touches the shared window-to-cache map from the render thread.
    if (change == ItemSceneChange) m_cache = data.window ? FmIconCache::of(data.window) : nullptr;
    if (change == ItemDevicePixelRatioHasChanged || change == ItemSceneChange) update();
}

QSGNode *FmIconItem::updatePaintNode(QSGNode *oldNode, UpdatePaintNodeData *) {
    auto *node = static_cast<QSGImageNode *>(oldNode);
    const qreal w = width(), h = height();
    if (m_name.isEmpty() || !m_cache || !(w > 0) || !(h > 0)) { delete node; return nullptr; }
    // As an Image with sourceSize did: whole device pixels, while the grid
    // scales by the exact ratio, like the former Canvas.
    const qreal ratio = window()->effectiveDevicePixelRatio();
    const QSize pixels = QSize(qRound(w * ratio), qRound(h * ratio)).expandedTo(QSize(1, 1));
    auto *texture = m_cache->texture(window(), {m_name, m_color.rgba(), pixels, QSizeF(ratio * w / 16, ratio * h / 16)});
    if (!node) {
        node = window()->createImageNode();
        node->setFiltering(QSGTexture::Linear);
        // The window's cache owns the texture; other items share it.
        node->setOwnsTexture(false);
    }
    if (node->texture() != texture) node->setTexture(texture);
    node->setRect(QRectF(0, 0, w, h));
    return node;
}

QImage FmIconItem::render(const QString &name, const QColor &color, const QSize &pixels, const QSizeF &scale) {
    QImage image(pixels, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    Sketch sketch;
    sketch.path.setFillRule(Qt::WindingFill);
    const bool stroked = draw(name, sketch);
    if (sketch.path.isEmpty()) return image;
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.scale(scale.width(), scale.height());
    // Square ends and mitred corners, as the design's line icons draw.
    if (stroked) painter.strokePath(sketch.path, QPen(color, 1.5, Qt::SolidLine, Qt::SquareCap, Qt::MiterJoin));
    else painter.fillPath(sketch.path, color);
    return image;
}

void FmIconItem::registerType() {
    static const int type = qmlRegisterType<FmIconItem>("FuckMail.Icons", 1, 0, "FmIconItem");
    Q_UNUSED(type);
    // Every process that loads the reader also needs the HTML item.
    FmHtmlView::registerType();
}
