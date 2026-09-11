#include "MailBackend.h"
#include "GuiSelfTest.h"

#include <QCommandLineParser>
#include <QDir>
#include <QDebug>
#include <QFile>
#include <QFont>
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
    app.setFont(QFont("Arial", 10));
    QQuickStyle::setStyle("Basic");
    QCommandLineParser parser;
    parser.setApplicationDescription("A local mail and calendar client — Phase 0 synthetic profile");
    parser.addHelpOption(); parser.addVersionOption();
    parser.addOption({"profile", "Disposable local profile directory.", "directory"});
    parser.addOption({"engine", "Path to fm-engine.", "executable"});
    parser.addOption({"smoke", "Exit successfully after the real engine supplies a conversation."});
    parser.addOption({"self-test", "Exercise GUI/CLI state against a new disposable profile."});
    parser.addOption({"screenshot", "Save the rendered window to a PNG after data loads.", "file"});
    parser.addOption({"screenshot-view", "Workspace for screenshot: mail, calendar or settings.", "view", "mail"});
    parser.process(app);
    const auto captureView = parser.value("screenshot-view");
    if (captureView != "mail" && captureView != "calendar" && captureView != "settings") parser.showHelp(1);
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
    MailBackend backend(profile, program);
    if (parser.isSet("smoke") || parser.isSet("self-test")) {
        QObject::connect(&backend, &MailBackend::errorChanged, &app, [&] {
            if (backend.error().isEmpty()) return;
            qCritical().noquote() << "GUI startup/integration error:" << backend.error();
            QCoreApplication::exit(5);
        });
    }
    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("backend", &backend);
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    QObject::connect(&backend, &MailBackend::shutdownReady, &app, &QCoreApplication::quit);
    bool captured = false;
    QObject::connect(&backend, &MailBackend::ready, &app, [&] {
        if (captured) return;
        captured = true;
        if (parser.isSet("smoke") || parser.isSet("screenshot")) QTimer::singleShot(800, &app, [&] {
            if (parser.isSet("screenshot")) {
                auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
                if (!window || !window->grabWindow().save(parser.value("screenshot"))) { QCoreApplication::exit(2); return; }
            }
            if (parser.isSet("smoke")) backend.shutdown();
        });
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
    }
    std::unique_ptr<GuiSelfTest> selfTest;
    if (parser.isSet("self-test") && !engine.rootObjects().isEmpty()) {
        selfTest = std::make_unique<GuiSelfTest>(backend, engine.rootObjects().first(), profile, program, &app);
        QTimer::singleShot(60000, &app, [] { QCoreApplication::exit(6); });
    }
    QTimer::singleShot(0, &backend, &MailBackend::start);
    const auto result = app.exec();
    if (smokeProfile) {
        // The engine owns its lifetime; never terminate it with the GUI.
        // Allow final-session checkpointing before removing this disposable profile.
        for (int i = 0; i < 30 && QFile::exists(QDir(profile).filePath("engine.json")); ++i) QThread::msleep(50);
        if (QFile::exists(QDir(profile).filePath("engine.json"))) smokeProfile->setAutoRemove(false);
    }
    return result;
}
