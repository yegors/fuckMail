#include "FmHtmlView.h"
#include "FmIconItem.h"
#include "HtmlLinks.h"
#include <QClipboard>
#include <QElapsedTimer>
#include <QGuiApplication>
#include <QImage>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSignalSpy>
#include <QTemporaryDir>
#include <QTest>
#include <QThread>
#include <cstdio>
#include <functional>

// Native checks for the HTML reader item: layout, placeholders, image states,
// scale-to-fit, links, selection and copy, tile bounds and fallback.
namespace {
int failures = 0;
int warnings = 0;
void check(bool ok, const char *what) {
    std::fprintf(stderr, "%s: %s\n", ok ? "ok" : "FAIL", what);
    if (!ok) ++failures;
}
bool await(const std::function<bool()> &done, int ms = 8000) {
    QElapsedTimer timer; timer.start();
    while (!done() && timer.elapsed() < ms) { QCoreApplication::processEvents(QEventLoop::AllEvents, 10); QThread::msleep(2); }
    return done();
}
QString png(const QTemporaryDir &dir, const QString &name, QSize size, QColor color) {
    QImage image(size, QImage::Format_ARGB32);
    image.fill(color);
    const auto path = dir.filePath(name);
    image.save(path, "PNG");
    return path;
}
int countNear(const QImage &image, const QRect &area, QColor target, int tolerance = 6) {
    int count = 0;
    const QRect bounded = area.intersected(image.rect());
    for (int y = bounded.top(); y <= bounded.bottom(); ++y)
        for (int x = bounded.left(); x <= bounded.right(); ++x) {
            const QColor c = image.pixelColor(x, y);
            if (std::abs(c.red() - target.red()) <= tolerance && std::abs(c.green() - target.green()) <= tolerance && std::abs(c.blue() - target.blue()) <= tolerance) ++count;
        }
    return count;
}
QImage grab(QQuickWindow *window) {
    // Let pending tiles reach the scene first.
    QCoreApplication::processEvents();
    return window->grabWindow().convertToFormat(QImage::Format_ARGB32);
}
QVariantMap content(const QString &body, const QVariantList &resources = {}) {
    return {{"html", "<!DOCTYPE html><html><head><style>p{margin:0 0 12px}</style></head><body style=\"margin:8px;font-family:Arial;color:#111\">" + body + "</body></html>"},
            {"resources", resources}};
}
QVariantMap image(const QString &state, const QString &path = {}) {
    QVariantMap value{{"state", state}};
    if (!path.isEmpty()) value["path"] = path;
    return value;
}
void linkRules() {
    using namespace HtmlLinks;
    check(normalizePattern(" https://WWW.Tracker.io/path?q#x ") == "tracker.io", "a pasted URL becomes its host without www");
    check(normalizePattern("*.doubleclick.net") == "*.doubleclick.net", "wildcard patterns are kept");
    check(normalizePattern("localhost").isEmpty() && normalizePattern("a b.com").isEmpty() && normalizePattern("x.123").isEmpty(), "non-domains are refused");
    check(normalizePattern("bücher.de") == "xn--bcher-kva.de", "international names are stored as ASCII");
    const QStringList patterns{"*.tracker.io", "news.example.com"};
    check(blockedBy(patterns, "cdn.tracker.io") == "*.tracker.io" && blockedBy(patterns, "tracker.io") == "*.tracker.io", "wildcards cover the domain and subdomains");
    check(blockedBy(patterns, "eviltracker.io").isEmpty(), "a suffix without a dot is not a subdomain");
    check(blockedBy(patterns, "www.news.example.com") == "news.example.com" && blockedBy(patterns, "a.news.example.com").isEmpty(), "exact patterns ignore www only");
    check(hostOf("https://WWW.Example.COM/a") == "example.com" && hostOf("not a url").isEmpty(), "link hosts are lowercase without www");
    check(label("https://www.example.com/a?b") == "example.com/a?b" && label("http://x.test/") == "x.test/", "plain-text labels drop the scheme and www");
}
}

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    qInstallMessageHandler([](QtMsgType type, const QMessageLogContext &, const QString &message) {
        std::fprintf(stderr, "%s\n", qPrintable(message));
        if (type != QtDebugMsg && type != QtInfoMsg) ++warnings;
    });
    linkRules();
    FmIconItem::registerType();
    QTemporaryDir dir;
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
import QtQuick
import FuckMail.Html
Flickable {
    id: flick
    width: 480; height: 640; clip: true
    contentHeight: view.height
    property alias view: view
    FmHtmlView { id: view; objectName: "view"; width: flick.width; height: implicitHeight; fontPixels: 16 }
}
)", QUrl("qrc:/HtmlViewTest.qml"));
    auto *root = qobject_cast<QQuickItem *>(component.create());
    check(root, "the test scene loads");
    if (!root) { std::fprintf(stderr, "%s\n", qPrintable(component.errorString())); return 1; }
    QQuickWindow window;
    window.resize(480, 640);
    root->setParentItem(window.contentItem());
    window.show();
    auto *view = root->findChild<FmHtmlView *>("view");
    auto *flick = root;
    const auto ready = [&] { return view->status() == "ready" && view->debugState()["pending_tiles"].toInt() == 0 && view->debugState()["tiles"].toInt() > 0; };

    // Layout, text runs and placeholders.
    view->setContent(content(
        "<p>Hello <b>reader</b>, see <a href=\"fm-link:1\">the news</a> and <a href=\"fm-link:2\">tracker</a>.</p>"
        "<p><img src=\"fm-res:1\" width=\"200\" height=\"100\" alt=\"Logo\"></p>"
        "<p><img src=\"fm-res:2\" alt=\"Photo\"></p><p>End of message.</p>",
        {QVariantMap{{"id", 1}, {"kind", "remote"}, {"alt", "Logo"}}, QVariantMap{{"id", 2}, {"kind", "remote"}, {"alt", "Photo"}}}));
    check(await(ready), "the document lays out and paints its tiles");
    check(view->text().contains("Hello") && view->text().contains("End of message.") && view->text().contains("reader"), "text runs cover the message");
    check(view->contentScale() == 1 && view->implicitHeight() > 150, "a narrow message is not scaled");
    check(!view->linkRects(1).isEmpty() && !view->linkRects(2).isEmpty(), "link boxes are published");
    auto frame = grab(&window);
    const QRect viewArea(0, 0, 480, int(view->implicitHeight()));
    check(countNear(frame, viewArea, QColor(0xf4, 0xf5, 0xf7), 2) + countNear(frame, viewArea, QColor(0xee, 0xf0, 0xf3), 2) > 2000, "hidden images draw striped placeholders");
    check(countNear(frame, viewArea, QColor(0x11, 0x11, 0x11), 40) > 100, "text is painted");
    const qreal before = view->implicitHeight();

    // A declared-size image loads without reflow; an undeclared one reflows.
    const auto red = png(dir, "red.png", {40, 30}, Qt::red);
    const auto blue = png(dir, "blue.png", {300, 150}, Qt::blue);
    view->setImages({{"1", image("ready", red)}, {"2", image("hidden")}});
    check(await([&] { return countNear(grab(&window), viewArea, Qt::red, 10) > 15000; }), "a ready image is drawn at its declared size");
    check(view->implicitHeight() == before, "declared sizes never reflow");
    view->setImages({{"1", image("ready", red)}, {"2", image("ready", blue)}});
    check(await([&] { return view->implicitHeight() > before + 100; }), "an undeclared image reflows when it loads");
    check(await([&] { return countNear(grab(&window), QRect(0, 0, 480, 640), Qt::blue, 10) > 30000; }), "the reflowed image is painted");
    view->setImages({{"1", image("blocked")}, {"2", image("failed")}});
    check(await([&] { return countNear(grab(&window), QRect(0, 0, 480, 640), Qt::red, 10) == 0; }), "a blocked image never shows its pixels");
    check(HtmlDocument::liveImageBytes() == 0, "replaced images are released");

    // Links: hover and click by id; blocked links stay clickable only through QML's check.
    QSignalSpy activated(view, &FmHtmlView::linkActivated);
    const auto linkBox = view->linkRects(1).first().toRectF();
    const QPoint linkPoint = view->mapToScene(linkBox.center()).toPoint();
    QTest::mouseMove(&window, linkPoint);
    check(await([&] { return view->hoveredLink() == 1; }), "hovering a link publishes it");
    QTest::mouseClick(&window, Qt::LeftButton, {}, linkPoint);
    check(await([&] { return activated.count() == 1 && activated.first().first().toInt() == 1; }), "clicking a link reports its id");
    QTest::mouseMove(&window, QPoint(470, 630));
    check(await([&] { return view->hoveredLink() == 0; }), "leaving the link clears it");

    // Selection and copy.
    view->selectAll();
    check(view->selectedText() == view->text(), "select all covers the document");
    view->copySelection();
    check(QGuiApplication::clipboard()->text() == view->text(), "copy puts the selected text on the clipboard");
    view->clearSelection();
    const auto helloBox = view->mapToScene(QPointF(9, linkBox.center().y())).toPoint();
    QTest::mousePress(&window, Qt::LeftButton, {}, helloBox);
    QTest::mouseMove(&window, helloBox + QPoint(30, 0));
    QTest::mouseMove(&window, helloBox + QPoint(60, 0));
    QTest::mouseRelease(&window, Qt::LeftButton, {}, helloBox + QPoint(60, 0));
    check(view->hasSelection() && view->selectedText().startsWith("Hell"), "dragging selects text from the press point");
    check(activated.count() == 1, "a drag is not a link click");

    // Blocked links restyle by rebuilding; text stays.
    const auto text = view->text();
    const qreal linkedHeight = view->implicitHeight();
    view->setBlockedLinks({2});
    check(view->status() == "loading" && view->implicitHeight() == linkedHeight, "restyling links keeps the height while it lays out");
    check(await(ready) && view->text() == text, "blocking a link keeps the document text");

    // Wide fixed layouts scale to fit, down to half size.
    view->setContent(content("<table width=\"900\" style=\"width:900px\"><tr><td>Wide newsletter</td></tr></table>"));
    check(await(ready) && std::abs(view->contentScale() - 480.0 / 900.0) < 0.02, "a 900 px layout scales to the column");
    view->setContent(content("<table width=\"1400\" style=\"width:1400px\"><tr><td>Wider still</td></tr></table>"));
    check(await(ready) && std::abs(view->contentScale() - 0.5) < 0.001, "scaling stops at half size");
    // Content laid out before has its final height on its first frame.
    const qreal wideHeight = view->implicitHeight();
    view->setContent(content("<p>Short</p><p>Two</p><p>Three</p><p>Four</p>"));
    check(await(ready) && view->implicitHeight() != wideHeight, "other content lays out");
    qreal firstFrame = -1;
    auto frames = QObject::connect(&window, &QQuickWindow::frameSwapped, &window, [&] { if (firstFrame < 0) firstFrame = view->implicitHeight(); });
    view->setContent(content("<table width=\"1400\" style=\"width:1400px\"><tr><td>Wider still</td></tr></table>"));
    check(await([&] { return firstFrame >= 0; }) && firstFrame == wideHeight, "content laid out before has its height on its first frame");
    QObject::disconnect(frames);
    check(await(ready) && view->implicitHeight() == wideHeight, "its layout keeps that height");

    // Declared cell widths are preferences: in a full-width table they shrink
    // to the column as browsers do, so the message is not scaled down.
    view->setContent(content("<table width=\"100%\"><tr><td width=\"600\">Left column text</td><td width=\"600\">Right column text</td></tr></table>"));
    check(await(ready) && view->contentScale() == 1, "fixed-width cells shrink to a full-width table");

    // Newsletter-style nesting: each level measures its content several times,
    // which grew exponentially before measurements were reused.
    QString nested = "<p>Innermost</p>";
    for (int i = 0; i < 12; ++i)
        nested = "<table width=\"100%\"><tr><td><div style=\"display:inline-block;width:100%\">" + nested + "</div></td></tr></table>";
    QElapsedTimer nestedTimer; nestedTimer.start();
    view->setContent(content(nested));
    check(await(ready) && view->text().contains("Innermost") && nestedTimer.elapsed() < 3000, "deeply nested tables lay out quickly");
    // A relatively positioned inline keeps its small offset however many times
    // its cell is laid out.
    QString price = "<span style=\"font-size:20px\"><sup style=\"font-size:20px;position:relative;top:2px;color:#ff0000\">$</sup>46</span>";
    for (int i = 0; i < 4; ++i) price = "<table><tr><td>" + price + "</td></tr></table>";
    view->setContent(content(price + "<div style=\"height:400px\"></div>"));
    check(await(ready), "the price lays out");
    const auto priceFrame = grab(&window);
    check(countNear(priceFrame, QRect(0, 0, 480, 60), Qt::red, 60) > 10 && countNear(priceFrame, QRect(0, 60, 480, 400), Qt::red, 60) == 0,
          "a relative superscript in a table stays on its line");
    // Crossing a media breakpoint on first layout keeps attribute styles and
    // the anonymous boxes of CSS tables.
    view->setContent(content("<style>@media (max-width:550px){.m{color:#111111}}</style>"
                             "<table width=\"200\" bgcolor=\"#ff0000\"><tr><td class=\"m\" height=\"60\">Cell</td></tr></table>"
                             "<div style=\"display:table\"><div style=\"display:table-cell\">CSS table text</div></div>"));
    check(await(ready) && view->text().contains("CSS table text"), "a CSS table keeps its content across a breakpoint");
    check(countNear(grab(&window), QRect(0, 0, 480, 120), Qt::red, 10) > 3000, "attribute styles survive a breakpoint");
    // A percentage height of a content-sized message computes to auto.
    view->setContent(content("<table width=\"100%\" height=\"100%\"><tr><td>Short notice</td></tr></table>"));
    check(await(ready) && view->implicitHeight() < 120, "a 100% tall table keeps its content height");
    // Long runs without spaces wrap after "/" and similar, or in pieces, as
    // browsers wrap logs and URLs; preformatted text wraps too.
    QString path = "https://example.com";
    for (int i = 0; i < 40; ++i) path += QStringLiteral("/segment%1").arg(i);
    view->setContent(content("<p>" + path + "</p><p>" + QString(300, 'x') + "</p><pre>" + QString("word ").repeated(200) + "</pre>"));
    check(await(ready) && view->contentScale() == 1, "long runs and preformatted lines wrap without scaling");
    // Copied text marks each wrapped line; the characters themselves are unchanged.
    const auto unwrapped = QString(view->text()).remove(QChar('\n'));
    check(unwrapped.contains(path) && unwrapped.contains(QString(300, 'x')), "wrapping pieces keep the text");

    // Long documents keep only tiles near the viewport.
    QString paragraphs;
    for (int i = 0; i < 600; ++i) paragraphs += QStringLiteral("<p>Paragraph %1 of a long message with enough words to wrap across the column width.</p>").arg(i);
    view->setContent(content(paragraphs));
    check(await(ready) && view->implicitHeight() > 20000, "a long message lays out");
    const int topTiles = view->debugState()["tiles"].toInt();
    check(topTiles > 0 && topTiles <= 4, "only tiles within a screen are painted");
    check(await([&] { return countNear(grab(&window), QRect(0, 0, 480, 640), QColor(0x11, 0x11, 0x11), 40) > 100; }), "the top of a long message is painted");
    flick->setProperty("contentY", 300);
    check(await([&] { return countNear(grab(&window), QRect(0, 0, 480, 640), QColor(0x11, 0x11, 0x11), 40) > 100; }), "a small scroll keeps the text painted");
    // Scrolling moves existing textures and paints only newly needed tiles.
    bool scrolled = true;
    for (const qreal y : {3000.0, 10000.0, 20000.0, view->implicitHeight() - 640}) {
        flick->setProperty("contentY", y);
        scrolled = await([&] { return view->debugState()["pending_tiles"].toInt() == 0
            && countNear(grab(&window), QRect(0, 0, 480, 640), QColor(0x11, 0x11, 0x11), 40) > 100; }) && scrolled;
    }
    check(scrolled, "scrolling through the message paints each new position");
    check(view->debugState()["tiles"].toInt() <= 4 && FmHtmlView::tileBytes() <= 32ll * 1024 * 1024, "tiles stay within the budget after scrolling");

    // Unreadable HTML falls back.
    view->setContent({{"html_path", dir.filePath("missing.html")}, {"resources", QVariantList{}}});
    check(await([&] { return view->status() == "failed"; }) && !view->failure().isEmpty(), "unreadable HTML reports a failure");

    delete root;
    check(await([] { return FmHtmlView::tileBytes() == 0; }), "destroying the view releases its tiles");
    check(warnings == 0, "no Qt warnings");
    std::fprintf(stderr, "%d failure(s)\n", failures);
    return failures ? 1 : 0;
}
