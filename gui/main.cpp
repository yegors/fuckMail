#include "MailBackend.h"
#include "GuiSelfTest.h"
#include "GuiSearchBenchmark.h"

#include <QCommandLineParser>
#include <QDir>
#include <QDebug>
#include <QFile>
#include <QFont>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QQuickStyle>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QThread>
#include <QTimer>

int main(int argc, char **argv) {
    QGuiApplication app(argc, argv);
    app.setApplicationName("fuckMail");
    app.setOrganizationName("fuckMail");
    app.setApplicationVersion("0.1.0");
    for (const auto &weight : {"Regular", "Medium", "SemiBold"})
        QFontDatabase::addApplicationFont(QString(":/qt/qml/FuckMail/fonts/Geist-%1.ttf").arg(weight));
    app.setFont(QFont("Geist", 10));
    QQuickStyle::setStyle("Basic");
    QCommandLineParser parser;
    parser.setApplicationDescription("A local mail and calendar client — Phase 1 synthetic profile");
    parser.addHelpOption(); parser.addVersionOption();
    parser.addOption({"profile", "Disposable local profile directory.", "directory"});
    parser.addOption({"engine", "Path to fm-engine.", "executable"});
    parser.addOption({"smoke", "Exit successfully after the real engine supplies a conversation."});
    parser.addOption({"self-test", "Exercise GUI/CLI state against a new disposable profile."});
    parser.addOption({"fixture-clock", "Use the deterministic fixture clock for development verification."});
    parser.addOption({"screenshot", "Save the rendered window to a PNG after data loads.", "file"});
    parser.addOption({"screenshot-view", "Workspace for screenshot: mail, calendar or settings.", "view", "mail"});
    parser.addOption({"screenshot-calendar-view", "Calendar layout for screenshot: day, week, month or agenda.", "view", "week"});
    parser.addOption({"screenshot-date", "Calendar date for screenshot (YYYY-MM-DD).", "date"});
    parser.addOption({"screenshot-settings-page", "Settings page for screenshot.", "page", "Accounts"});
    parser.addOption({"screenshot-width", "Window width in logical pixels for visual checks.", "pixels", "1440"});
    parser.addOption({"screenshot-height", "Window height in logical pixels for visual checks.", "pixels", "900"});
    parser.addOption({"screenshot-reply", "Expand the current conversation's inline reply for a screenshot."});
    parser.addOption({"benchmark-search-output", "Measure actual QML input to rendered first-page results on a retained generated 100k profile; writes a JSON report.", "file"});
    parser.process(app);
    if (parser.isSet("benchmark-search-output") && (!parser.isSet("profile") || parser.isSet("smoke") || parser.isSet("self-test"))) parser.showHelp(1);
    const auto captureView = parser.value("screenshot-view");
    if (captureView != "mail" && captureView != "calendar" && captureView != "settings") parser.showHelp(1);
    if (!QStringList{"day", "week", "month", "agenda"}.contains(parser.value("screenshot-calendar-view"))) parser.showHelp(1);
    if (!QStringList{"Accounts", "Mail", "Calendar", "Shortcuts", "Diagnostics"}.contains(parser.value("screenshot-settings-page"))) parser.showHelp(1);
    if (parser.isSet("screenshot-date") && !QDate::fromString(parser.value("screenshot-date"), Qt::ISODate).isValid()) parser.showHelp(1);
    auto profile = parser.value("profile");
    std::unique_ptr<QTemporaryDir> smokeProfile;
    if (parser.isSet("self-test") || (profile.isEmpty() && parser.isSet("smoke"))) {
        smokeProfile = std::make_unique<QTemporaryDir>(QDir::tempPath() + "/fuckmail-gui-smoke-XXXXXX");
        if (!smokeProfile->isValid()) return 4;
        profile = QDir(smokeProfile->path()).filePath("profile");
    }
    if (profile.isEmpty()) profile = qEnvironmentVariable("FM_PROFILE");
    if (profile.isEmpty()) {
#ifdef Q_OS_MACOS
        auto base = QDir::homePath() + "/Library/Application Support";
#elif defined(Q_OS_WIN)
        auto base = qEnvironmentVariable("LOCALAPPDATA");
#else
        auto base = qEnvironmentVariable("XDG_DATA_HOME", QDir::homePath() + "/.local/share");
#endif
        profile = QDir(base).filePath("fuckMail/demo-v1");
    }
    profile = QDir(profile).absolutePath();
    auto program = parser.value("engine");
    if (program.isEmpty()) program = qEnvironmentVariable("FM_ENGINE_PATH");
    if (program.isEmpty()) program = QDir(QCoreApplication::applicationDirPath()).filePath("fm-engine"
#ifdef Q_OS_WIN
        ".exe"
#endif
    );
    MailBackend backend(profile, program, nullptr, parser.isSet("fixture-clock") || parser.isSet("self-test"));
    if (parser.isSet("smoke") || parser.isSet("self-test")) {
        QObject::connect(&backend, &MailBackend::errorChanged, &app, [&] {
            if (backend.error().isEmpty()) return;
            if (parser.isSet("self-test") && backend.property("selfTestExpectedError").toBool()) return;
            qCritical().noquote() << "GUI startup/integration error:" << backend.error();
            QCoreApplication::exit(5);
        });
    }
    // Render-thread measurement callbacks outlive the event loop on an error
    // exit. Keep their receiver alive until the QML window stops rendering.
    std::unique_ptr<GuiSearchBenchmark> searchBenchmark;
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("backend", &backend);
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    QObject::connect(&backend, &MailBackend::shutdownReady, &app, &QCoreApplication::quit);
    bool captured = false;
    QTimer captureTimer;
    captureTimer.setInterval(100);
    int settledTicks = 0;
    bool screenshotReplyOpened = false;
    QObject::connect(&captureTimer, &QTimer::timeout, &app, [&] {
        if (!backend.connected() || backend.loading() || backend.draft().isEmpty()
            || !backend.property("calendarReady").toBool() || !backend.property("timelineReady").toBool()) { settledTicks = 0; return; }
        if (parser.isSet("screenshot-reply") && !screenshotReplyOpened) {
            auto root = engine.rootObjects().value(0);
            auto mail = root ? root->findChild<QObject *>("mailWorkspace") : nullptr;
            if (!mail || !QMetaObject::invokeMethod(mail, "openReply")) { QCoreApplication::exit(2); return; }
            screenshotReplyOpened = true; settledTicks = 0; return;
        }
        if (++settledTicks < 3) return;
        captureTimer.stop();
        if (parser.isSet("screenshot")) {
            auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
            if (!window || !window->grabWindow().save(parser.value("screenshot"))) { QCoreApplication::exit(2); return; }
        }
        if (parser.isSet("smoke")) backend.shutdown();
    });
    QObject::connect(&backend, &MailBackend::ready, &app, [&] {
        if (captured) return;
        captured = true;
        if (parser.isSet("screenshot")) {
            backend.setCalendarView(parser.value("screenshot-calendar-view"));
            if (parser.isSet("screenshot-date")) backend.setCalendarDate(parser.value("screenshot-date"));
        }
        if (parser.isSet("smoke") || parser.isSet("screenshot")) captureTimer.start();
    });
    if (parser.isSet("smoke")) QTimer::singleShot(40000, &app, [] {
        qCritical() << "GUI smoke timed out before loading a conversation.";
        QCoreApplication::exit(3);
    });
    engine.loadFromModule("FuckMail", "Main");
    if (parser.isSet("screenshot") && !engine.rootObjects().isEmpty()) {
        auto root = engine.rootObjects().first();
        root->setProperty("workspace", captureView == "calendar" ? "calendar" : "mail");
        root->setProperty("settingsOpen", captureView == "settings");
        root->setProperty("width", qBound(1000, parser.value("screenshot-width").toInt(), 3840));
        root->setProperty("height", qBound(650, parser.value("screenshot-height").toInt(), 2160));
        if (auto settings = root->findChild<QObject *>("settingsView")) settings->setProperty("page", parser.value("screenshot-settings-page"));
    }
    std::unique_ptr<GuiSelfTest> selfTest;
    if (parser.isSet("benchmark-search-output") && !engine.rootObjects().isEmpty()) {
        auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        if (!window) return 7;
        searchBenchmark = std::make_unique<GuiSearchBenchmark>(backend, window, parser.value("benchmark-search-output"), &app);
    }
    if (parser.isSet("self-test") && !engine.rootObjects().isEmpty()) {
        selfTest = std::make_unique<GuiSelfTest>(backend, engine.rootObjects().first(), profile, program, &app);
        QTimer::singleShot(120000, &app, [] { QCoreApplication::exit(6); });
    }
    QTimer::singleShot(0, &backend, &MailBackend::start);
    const auto result = app.exec();
    if (searchBenchmark) searchBenchmark->stopForShutdown();
    const bool selfTestCompleted = backend.property("selfTestCompleted").toBool();
    const bool restartVerified = result != 0 || !selfTest || !selfTestCompleted || selfTest->verifyRestart();
    if (smokeProfile) {
        // The engine owns its lifetime; never terminate it with the GUI.
        // Allow final-session checkpointing before removing this disposable profile.
        for (int i = 0; i < 30 && QFile::exists(QDir(profile).filePath("engine.json")); ++i) QThread::msleep(50);
        if (QFile::exists(QDir(profile).filePath("engine.json"))) smokeProfile->setAutoRemove(false);
    }
    if (result == 0 && parser.isSet("self-test") && !selfTestCompleted) {
        qCritical() << "GUI self-test exited before its final independent recovery check.";
        return 5;
    }
    if (!restartVerified) return 5;
    return result;
}
