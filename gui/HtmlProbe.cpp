#include "HtmlDocument.h"
#include <QCommandLineParser>
#include <QDir>
#include <QElapsedTimer>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QImageReader>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPainter>
#include <cstdio>

// Developer tool: lays out sanitised message HTML the way FmHtmlView does and
// reports parse and layout time, size and scale per file, optionally saving a
// PNG of the top of each message or listing its painted text. Input is a directory of `message_html`
// results (`NNNN.html`, plus an optional `index.json` whose `resources` give
// cached image paths). It makes no network or engine request. Keep real mail
// it reads outside the repository.
namespace {
constexpr qreal minimumScale = 0.5;

QHash<int, HtmlImage> imagesFor(const QJsonArray &resources, bool loadImages) {
    QHash<int, HtmlImage> images;
    for (const auto &value : resources) {
        const auto resource = value.toObject();
        HtmlImage image;
        image.alt = resource["alt"].toString();
        const auto kind = resource["kind"].toString();
        const auto path = resource["image"].toObject()["path"].toString();
        if (kind == "insecure") image.state = HtmlImage::Insecure;
        else if (!path.isEmpty() && (loadImages || kind != "remote")) {
            QImageReader reader(path, "png");
            image.image = reader.read().convertToFormat(QImage::Format_ARGB32_Premultiplied);
            image.size = image.image.size();
            image.state = image.image.isNull() ? HtmlImage::Failed : HtmlImage::Ready;
        }
        images.insert(resource["id"].toInt(), image);
    }
    return images;
}
}

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    QCommandLineParser parser;
    parser.addHelpOption();
    parser.addPositionalArgument("input", "Directory of NNNN.html files, or single files");
    parser.addOptions({{"width", "Column width in CSS pixels", "px", "720"},
                       {"png", "Save a PNG of each message's top into this directory", "dir"},
                       {"height", "PNG height limit", "px", "2400"},
                       {"images", "Show cached remote images as loaded"},
                       {"text", "Include the painted text"},
                       {"font", "Default font size in pixels", "px", "16"}});
    parser.process(app);
    const qreal width = parser.value("width").toDouble(), font = parser.value("font").toDouble();
    QJsonObject index;
    QStringList files;
    for (const auto &input : parser.positionalArguments()) {
        const QFileInfo info(input);
        if (info.isDir()) {
            QDir dir(input);
            for (const auto &name : dir.entryList({"*.html"}, QDir::Files, QDir::Name))
                if (!name.contains(".orig.") && !name.contains(".chromium.")) files << dir.filePath(name);
            QFile file(dir.filePath("index.json"));
            if (file.open(QIODevice::ReadOnly))
                for (const auto &entry : QJsonDocument::fromJson(file.readAll()).array())
                    index.insert(entry.toObject()["file"].toString(), entry);
        } else files << input;
    }
    if (parser.isSet("png")) QDir().mkpath(parser.value("png"));
    QJsonArray report;
    for (const auto &path : files) {
        QFile file(path);
        if (!file.open(QIODevice::ReadOnly)) continue;
        const auto html = file.readAll();
        const auto name = QFileInfo(path).completeBaseName();
        const auto entry = index.value(name).toObject();
        const auto images = imagesFor(entry["resources"].toArray(), parser.isSet("images"));
        QElapsedTimer timer;
        timer.start();
        HtmlDocument document(html, font, {}, images);
        const qint64 parseMs = timer.elapsed();
        timer.restart();
        QSizeF size = document.layout(width);
        qreal scale = 1;
        if (size.width() > width + 0.5) {
            scale = std::max(minimumScale, width / size.width());
            size = document.layout(std::floor(width / scale));
        }
        const qint64 layoutMs = timer.elapsed();
        QJsonObject row{{"file", name}, {"subject", entry["subject"]}, {"bytes", html.size()}, {"valid", document.valid()},
                        {"parse_ms", parseMs}, {"layout_ms", layoutMs}, {"width", size.width()}, {"height", size.height()},
                        {"scale", scale}};
        if (parser.isSet("text") && document.valid()) row["text"] = document.snapshot().text;
        if (parser.isSet("png") && document.valid()) {
            timer.restart();
            const qreal shown = std::min(size.height() * scale, parser.value("height").toDouble());
            QImage image(QSize(int(width), std::max(1, int(shown))), QImage::Format_ARGB32_Premultiplied);
            image.fill(Qt::white);
            QPainter painter(&image);
            painter.setRenderHint(QPainter::TextAntialiasing);
            painter.scale(scale, scale);
            document.paint(painter, QRectF(0, 0, width / scale, shown / scale));
            painter.end();
            row["paint_ms"] = timer.elapsed();
            image.save(QDir(parser.value("png")).filePath(name + ".png"));
        }
        std::fprintf(stdout, "%s\n", QJsonDocument(row).toJson(QJsonDocument::Compact).constData());
        std::fflush(stdout);
        report.append(row);
    }
    return 0;
}
