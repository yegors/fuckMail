#include "WindowPlacement.h"
#include <QFile>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QQmlApplicationEngine>
#include <QQuickWindow>
#include <QScreen>
#include <QTemporaryDir>
#include <QtTest>

namespace {
QJsonObject saved(const QTemporaryDir &dir) {
    QFile file(dir.filePath("window.json"));
    return file.open(QIODevice::ReadOnly) ? QJsonDocument::fromJson(file.readAll()).object() : QJsonObject{};
}
QQuickWindow *load(QQmlApplicationEngine &engine, const QVariantMap &initial) {
    engine.setInitialProperties(initial);
    engine.loadData("import QtQuick\nWindow { width: 640; height: 400; visible: true }");
    return qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
}
}

class WindowPlacementTest : public QObject {
    Q_OBJECT
private slots:
    void fitsTheSavedPlaceToTheScreens() {
        const QList<QRect> screens{{0, 0, 1920, 1040}, {1920, 0, 2560, 1400}};
        const QSize minimum(1000, 650);
        // On a screen: unchanged, including on the second monitor.
        QCOMPARE(WindowPlacement::fit({100, 80, 1440, 900}, screens, minimum), QRect(100, 80, 1440, 900));
        QCOMPARE(WindowPlacement::fit({2100, 50, 1600, 1000}, screens, minimum), QRect(2100, 50, 1600, 1000));
        // The monitor it was on is gone: default placement.
        QCOMPARE(WindowPlacement::fit({5000, 100, 1440, 900}, screens, minimum), QRect());
        QCOMPARE(WindowPlacement::fit({100, -600, 1440, 900}, screens, minimum), QRect());
        // Larger than its screen: capped and kept inside it.
        QCOMPARE(WindowPlacement::fit({10, 10, 2400, 1300}, {{0, 0, 1920, 1040}}, minimum), QRect(0, 0, 1920, 1040));
        // Hanging off an edge with the title still reachable: pulled back in.
        QCOMPARE(WindowPlacement::fit({1500, 100, 1440, 900}, {{0, 0, 1920, 1040}}, minimum), QRect(480, 100, 1440, 900));
        // Below the minimum on a small screen: the screen wins.
        QCOMPARE(WindowPlacement::fit({0, 0, 1200, 700}, {{0, 0, 800, 600}}, minimum), QRect(0, 0, 800, 600));
        QCOMPARE(WindowPlacement::fit({}, screens, minimum), QRect());
    }

    void reopensWhereItWasLeft() {
        QTemporaryDir dir; QVERIFY(dir.isValid());
        const auto area = QGuiApplication::primaryScreen()->availableGeometry();
        const QRect place(area.x() + 30, area.y() + 40, qMin(500, area.width() - 60), qMin(320, area.height() - 60));
        {
            // Nothing saved yet: the window keeps its own default.
            WindowPlacement placement(dir.path(), {200, 150});
            QVERIFY(placement.initialProperties().isEmpty());
            QQmlApplicationEngine engine;
            auto window = load(engine, placement.initialProperties()); QVERIFY(window);
            placement.track(window);
            QVERIFY(QTest::qWaitForWindowExposed(window));
            window->setGeometry(place);
            QTRY_COMPARE(saved(dir).value("x").toInt(), place.x());
            const auto object = saved(dir);
            QCOMPARE(QRect(object.value("x").toInt(), object.value("y").toInt(), object.value("width").toInt(), object.value("height").toInt()), window->geometry());
            QCOMPARE(object.value("maximized").toBool(), false);
            // Minimized keeps the last normal place.
            window->showMinimized(); QTest::qWait(400);
            QCOMPARE(saved(dir).value("x").toInt(), place.x());
        }
        {
            // The next launch opens there.
            WindowPlacement placement(dir.path(), {200, 150});
            const auto initial = placement.initialProperties();
            QCOMPARE(initial.value("x").toInt(), place.x());
            QCOMPARE(initial.value("width").toInt(), place.width());
            QVERIFY(!initial.contains("visibility"));
            QQmlApplicationEngine engine;
            auto window = load(engine, initial); QVERIFY(window);
            QVERIFY(QTest::qWaitForWindowExposed(window));
            QCOMPARE(window->position(), place.topLeft());
            QCOMPARE(window->size(), place.size());
            placement.track(window);
            // Maximized is remembered with the normal place underneath it.
            window->showMaximized();
            QTRY_VERIFY(saved(dir).value("maximized").toBool());
            QCOMPARE(saved(dir).value("x").toInt(), place.x());
            QCOMPARE(saved(dir).value("width").toInt(), place.width());
        }
        {
            WindowPlacement placement(dir.path(), {200, 150});
            const auto initial = placement.initialProperties();
            QCOMPARE(initial.value("visibility").toInt(), int(QWindow::Maximized));
            QCOMPARE(initial.value("x").toInt(), place.x());
            QQmlApplicationEngine engine;
            auto window = load(engine, initial); QVERIFY(window);
            QTRY_VERIFY(window->windowStates() & Qt::WindowMaximized);
        }
        // A profile the engine has not created yet is left alone.
        WindowPlacement missing(dir.filePath("not-created"), {200, 150});
        missing.save();
        QVERIFY(!QFile::exists(dir.filePath("not-created")));
    }
};

QTEST_MAIN(WindowPlacementTest)
#include "WindowPlacementTest.moc"
