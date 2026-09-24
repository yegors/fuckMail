#include "MailBackend.h"
#include "FmIconItem.h"
#include "DebugConsole.h"
#include "NotificationPresenter.h"
#include "GuiSelfTest.h"
#include "GuiCalendarSelfTest.h"
#include "GuiCompositionSelfTest.h"
#include "GuiGoogleDraftSnapshotSelfTest.h"
#include "GuiGoogleDraftActionsSelfTest.h"
#include "GuiGoogleCalendarSelfTest.h"
#include "GuiAccountLifecycleSelfTest.h"
#include "GuiSearchBenchmark.h"
#include "GuiInteractionBenchmark.h"
#include "GuiResizeBenchmark.h"
#include "GuiCalendarBenchmark.h"

#include <QCommandLineParser>
#include <QDir>
#include <QDebug>
#include <QElapsedTimer>
#include <QFile>
#include <QFont>
#include <QFontDatabase>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMouseEvent>
#include <QQuickItem>
#include <functional>
#include <QApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QQuickStyle>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QThread>
#include <QTimer>

int main(int argc, char **argv) {
#ifdef Q_OS_WIN
    // Qt 6.11.2's DXGI VBlank notifier can leave an exposed window without
    // update requests on this host. Use Qt's timer fallback; D3D rendering
    // and presentation vsync remain enabled. Preserve an explicit override.
    if (!qEnvironmentVariableIsSet("QT_D3D_NO_VBLANK_THREAD"))
        qputenv("QT_D3D_NO_VBLANK_THREAD", "1");
#endif
    QApplication app(argc, argv);
    DebugConsole debugConsole;
    debugConsole.captureQtMessages();
    app.setApplicationName("fuckMail");
    app.setOrganizationName("fuckMail");
    app.setApplicationVersion("0.1.0");
    for (const auto &weight : {"Regular", "Medium", "SemiBold"})
        QFontDatabase::addApplicationFont(QString(":/qt/qml/FuckMail/fonts/Geist-%1.ttf").arg(weight));
    app.setFont(QFont("Geist", 10));
    QQuickStyle::setStyle("Basic");
    QCommandLineParser parser;
    parser.setApplicationDescription("A local mail and calendar client");
    parser.addHelpOption(); parser.addVersionOption();
    parser.addOption({"profile", "Local profile directory.", "directory"});
    parser.addOption({"live-profile", "Create or open an explicitly selected live profile without demo content."});
    parser.addOption({"demo-profile", "Explicitly open synthetic demo content in a separate profile."});
    parser.addOption({"self-test-live-empty", "Verify a new empty live profile and close without connecting Google."});
    parser.addOption({"engine", "Path to fm-engine.", "executable"});
    parser.addOption({"smoke", "Exit successfully after the real engine supplies a conversation."});
    parser.addOption({"self-test", "Exercise GUI/CLI state against a new disposable profile."});
    parser.addOption({"self-test-attachments", "Run only native local-attachment integration with --self-test."});
    parser.addOption({"self-test-calendar-restart", "Verify the native calendar after a self-test process restart.", "file"});
    parser.addOption({"self-test-composition-restart", "Verify an independently saved draft after a self-test process restart.", "file"});
    parser.addOption({"self-test-startup-workspace", "Verify the saved initial workspace in a fresh native process.", "mail|calendar"});
    parser.addOption({"fixture-clock", "Use the deterministic fixture clock for development verification."});
    parser.addOption({"screenshot", "Save the rendered window to a PNG after data loads.", "file"});
    parser.addOption({"screenshot-view", "Workspace for screenshot: mail, calendar or settings.", "view", "mail"});
    parser.addOption({"screenshot-calendar-view", "Calendar layout for screenshot: day, week, month or agenda.", "view", "week"});
    parser.addOption({"screenshot-date", "Calendar date for screenshot (YYYY-MM-DD).", "date"});
    parser.addOption({"screenshot-settings-page", "Settings page for screenshot.", "page", "General"});
    parser.addOption({"screenshot-width", "Window width in logical pixels for visual checks.", "pixels", "1440"});
    parser.addOption({"screenshot-height", "Window height in logical pixels for visual checks.", "pixels", "900"});
    parser.addOption({"screenshot-reply", "Expand the current conversation's inline reply for a screenshot."});
    parser.addOption({"screenshot-reply-bottom", "Scroll the expanded reply to its action footer before capture."});
    parser.addOption({"screenshot-set", "Set objectName.property=JSON on a named item before capture (repeatable).", "assignment"});
    parser.addOption({"screenshot-invoke", "Call a no-argument method objectName.method before capture (repeatable, in order).", "call"});
    parser.addOption({"screenshot-click", "Left-click the centre of a named visible item before capture (repeatable, after assignments and calls).", "objectName"});
    parser.addOption({"benchmark-search-output", "Measure actual QML input to rendered first-page results on a retained generated 100k profile; writes a JSON report.", "file"});
    parser.addOption({"benchmark-navigation-output", "Measure native mailbox and conversation input to correct content paint on a disposable fixture profile.", "file"});
    parser.addOption({"benchmark-resize-output", "Measure native resize frames and retain separate rendering captures on a disposable fixture profile.", "file"});
    parser.addOption({"benchmark-calendar-output", "Measure native Calendar view and date input to correct grid paint on a disposable fixture profile.", "file"});
    parser.addOption({"google-drafts-self-test", "Verify cached imported Google drafts without starting provider sync.", "report"});
    parser.addOption({"google-drafts-expectation", "Independently verified cached draft/source expectation for the dedicated walkthrough.", "file"});
    parser.addOption({"google-draft-actions-self-test", "Verify explicit Google draft source actions on an isolated provider fixture.", "report"});
    parser.addOption({"google-draft-actions-expectation", "Independent exact account/source action expectation.", "file"});
    parser.addOption({"google-calendar-self-test", "Verify ordinary Calendar guest edits/create/delete on an isolated provider fixture.", "report"});
    parser.addOption({"google-calendar-expectation", "Independent exact Calendar action expectation.", "file"});
    parser.addOption({"account-lifecycle-self-test", "Verify native account addition, archive removal and restoration on an isolated fixture.", "report"});
    parser.addOption({"account-lifecycle-expectation", "Independent generated account lifecycle expectation.", "file"});
    parser.process(app);
    const bool lifecycleTest = parser.isSet("account-lifecycle-self-test");
    const bool calendarTest = parser.isSet("google-calendar-self-test");
    const bool actionsTest = parser.isSet("google-draft-actions-self-test");
    const bool snapshotTest = parser.isSet("google-drafts-self-test");
    if (lifecycleTest != parser.isSet("account-lifecycle-expectation")
        || (lifecycleTest && (calendarTest || actionsTest || snapshotTest || !parser.isSet("profile") || !parser.isSet("engine")
            || parser.isSet("smoke") || parser.isSet("self-test") || parser.isSet("self-test-live-empty")
            || parser.isSet("screenshot") || parser.isSet("fixture-clock") || parser.isSet("self-test-startup-workspace")
            || parser.isSet("self-test-calendar-restart") || parser.isSet("self-test-composition-restart")
            || parser.isSet("benchmark-search-output") || parser.isSet("benchmark-navigation-output")
            || parser.isSet("benchmark-resize-output") || parser.isSet("benchmark-calendar-output")))) parser.showHelp(1);
    if (calendarTest != parser.isSet("google-calendar-expectation")
        || (calendarTest && (actionsTest || snapshotTest || !parser.isSet("profile") || !parser.isSet("engine")
            || parser.isSet("smoke") || parser.isSet("self-test") || parser.isSet("self-test-live-empty")
            || parser.isSet("screenshot") || parser.isSet("fixture-clock") || parser.isSet("self-test-startup-workspace")
            || parser.isSet("self-test-calendar-restart") || parser.isSet("self-test-composition-restart")
            || parser.isSet("benchmark-search-output") || parser.isSet("benchmark-navigation-output")
            || parser.isSet("benchmark-resize-output") || parser.isSet("benchmark-calendar-output")))) parser.showHelp(1);
    if (actionsTest != parser.isSet("google-draft-actions-expectation")
        || (actionsTest && (snapshotTest || !parser.isSet("profile") || !parser.isSet("engine")
            || parser.isSet("smoke") || parser.isSet("self-test") || parser.isSet("self-test-live-empty")
            || parser.isSet("screenshot") || parser.isSet("fixture-clock") || parser.isSet("self-test-startup-workspace")
            || parser.isSet("self-test-calendar-restart") || parser.isSet("self-test-composition-restart")
            || parser.isSet("benchmark-search-output") || parser.isSet("benchmark-navigation-output")
            || parser.isSet("benchmark-resize-output") || parser.isSet("benchmark-calendar-output")))) parser.showHelp(1);
    if (snapshotTest != parser.isSet("google-drafts-expectation")
        || (snapshotTest && (!parser.isSet("profile") || !parser.isSet("engine")
            || parser.isSet("smoke") || parser.isSet("self-test") || parser.isSet("self-test-live-empty")
            || parser.isSet("screenshot") || parser.isSet("fixture-clock")
            || parser.isSet("self-test-startup-workspace") || parser.isSet("self-test-calendar-restart")
            || parser.isSet("self-test-composition-restart") || parser.isSet("benchmark-search-output")
            || parser.isSet("benchmark-navigation-output") || parser.isSet("benchmark-resize-output") || parser.isSet("benchmark-calendar-output")))) parser.showHelp(1);
    if (parser.isSet("self-test-attachments") && (!parser.isSet("self-test") || parser.isSet("profile")
        || parser.isSet("smoke") || parser.isSet("screenshot") || parser.isSet("live-profile")
        || parser.isSet("self-test-live-empty") || parser.isSet("benchmark-search-output")
        || parser.isSet("benchmark-navigation-output") || parser.isSet("benchmark-resize-output") || parser.isSet("benchmark-calendar-output"))) parser.showHelp(1);
    if (parser.isSet("demo-profile") && (parser.isSet("live-profile") || parser.isSet("self-test-live-empty") || lifecycleTest || calendarTest || actionsTest || snapshotTest)) parser.showHelp(1);
    if (parser.isSet("live-profile") && (!parser.isSet("profile") || parser.isSet("self-test") || parser.isSet("fixture-clock"))) parser.showHelp(1);
    if (parser.isSet("self-test-live-empty") && (parser.isSet("self-test") || parser.isSet("fixture-clock") || parser.isSet("smoke") || parser.isSet("screenshot"))) parser.showHelp(1);
    if (parser.isSet("self-test-startup-workspace") && (!parser.isSet("smoke") || !parser.isSet("profile") || parser.isSet("self-test")
        || parser.isSet("self-test-calendar-restart") || parser.isSet("self-test-composition-restart") || parser.isSet("screenshot")
        || !QStringList{"mail", "calendar"}.contains(parser.value("self-test-startup-workspace")))) parser.showHelp(1);
    if (parser.isSet("self-test-calendar-restart") && (!parser.isSet("smoke") || !parser.isSet("profile") || parser.isSet("self-test"))) parser.showHelp(1);
    if (parser.isSet("self-test-composition-restart") && (!parser.isSet("smoke") || !parser.isSet("profile") || parser.isSet("self-test") || parser.isSet("self-test-calendar-restart"))) parser.showHelp(1);
    if (parser.isSet("benchmark-search-output") && (!parser.isSet("profile") || parser.isSet("smoke") || parser.isSet("self-test"))) parser.showHelp(1);
    const int benchmarkCount = int(parser.isSet("benchmark-search-output")) + int(parser.isSet("benchmark-navigation-output")) + int(parser.isSet("benchmark-resize-output"))
        + int(parser.isSet("benchmark-calendar-output"));
    if (benchmarkCount && (benchmarkCount != 1 || !parser.isSet("profile") || parser.isSet("smoke") || parser.isSet("self-test") || parser.isSet("screenshot"))) parser.showHelp(1);
    const auto captureView = parser.value("screenshot-view");
    if (captureView != "mail" && captureView != "calendar" && captureView != "settings") parser.showHelp(1);
    if (!QStringList{"day", "week", "month", "agenda"}.contains(parser.value("screenshot-calendar-view"))) parser.showHelp(1);
    if (!QStringList{"General", "Accounts", "Local LLM", "Mail", "Calendar", "Shortcuts", "Diagnostics"}.contains(parser.value("screenshot-settings-page"))) parser.showHelp(1);
    if (parser.isSet("screenshot-date") && !QDate::fromString(parser.value("screenshot-date"), Qt::ISODate).isValid()) parser.showHelp(1);
    auto profile = parser.value("profile");
    const bool demoProfile = parser.isSet("demo-profile") || parser.isSet("self-test")
        || parser.isSet("smoke") || parser.isSet("fixture-clock");
    std::unique_ptr<QTemporaryDir> smokeProfile;
    if (parser.isSet("self-test") || (profile.isEmpty() && (parser.isSet("smoke") || parser.isSet("self-test-live-empty")))) {
        smokeProfile = std::make_unique<QTemporaryDir>(QDir::tempPath() + "/fuckmail-gui-smoke-XXXXXX");
        if (!smokeProfile->isValid()) return 4;
        profile = QDir(smokeProfile->path()).filePath("profile");
    }
    if (profile.isEmpty()) profile = qEnvironmentVariable("FM_PROFILE");
    const bool selectedProfile = !profile.isEmpty();
    if (profile.isEmpty()) {
#ifdef Q_OS_MACOS
        auto base = QDir::homePath() + "/Library/Application Support";
#elif defined(Q_OS_WIN)
        auto base = qEnvironmentVariable("LOCALAPPDATA");
#else
        auto base = qEnvironmentVariable("XDG_DATA_HOME", QDir::homePath() + "/.local/share");
#endif
        if (base.isEmpty()) { qCritical() << "Cannot locate the user data directory; pass --profile."; return 2; }
        profile = QDir(base).filePath(demoProfile ? "fuckMail/demo-v1" : "fuckMail/default");
    }
    profile = QDir(profile).absolutePath();
    auto program = parser.value("engine");
    if (program.isEmpty()) program = qEnvironmentVariable("FM_ENGINE_PATH");
    if (program.isEmpty()) program = QDir(QCoreApplication::applicationDirPath()).filePath("fm-engine"
#ifdef Q_OS_WIN
        ".exe"
#endif
    );
    const bool requireLive = parser.isSet("live-profile") || parser.isSet("self-test-live-empty")
        || snapshotTest || actionsTest || calendarTest || lifecycleTest || (!selectedProfile && !demoProfile);
    MailBackend backend(profile, program, nullptr, parser.isSet("fixture-clock") || parser.isSet("self-test"), requireLive);
    debugConsole.attach(&backend);
    debugConsole.setLogDirectory(profile);
    debugConsole.identifyRuntime(program);
    backend.setProperty("demoProfileRequested", demoProfile && !requireLive);
    NotificationPresenter notifications(&backend);
    if (parser.isSet("self-test")) {
        backend.setProperty("verificationFixture", "snapshot");
        backend.setProperty("selfTestArrivalReport", QDir(profile).filePath("verification-ingestion.json"));
    }
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
    std::unique_ptr<GuiInteractionBenchmark> interactionBenchmark;
    std::unique_ptr<GuiResizeBenchmark> resizeBenchmark;
    std::unique_ptr<GuiCalendarBenchmark> calendarBenchmark;
    QQmlApplicationEngine engine; FmIconItem::registerType();
    engine.rootContext()->setContextProperty("backend", &backend);
    engine.rootContext()->setContextProperty("debugDiagnostics", &debugConsole);
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app, [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    QObject::connect(&backend, &MailBackend::shutdownReady, &app, &QCoreApplication::quit);
    QTimer emptyLiveCheck;
    QElapsedTimer emptyLiveStable;
    QString emptyLiveDensity;
    qlonglong emptyLiveInitialRevision = -1;
    if (parser.isSet("self-test-live-empty")) {
        emptyLiveCheck.setInterval(50);
        QObject::connect(&emptyLiveCheck, &QTimer::timeout, &app, [&] {
            if (!backend.error().isEmpty()) { qCritical().noquote() << backend.error(); app.exit(5); return; }
            if (!backend.connected() || !backend.diagnostics().contains("synthetic") || !backend.calendarReady() || !backend.timelineReady()) return;
            if (backend.syntheticProfile() || backend.diagnostics().value("synthetic").toBool()
                || backend.diagnostics().value("fixture_revision").toInt() != 0
                || !backend.accounts().isEmpty() || !backend.conversations().isEmpty()
                || !backend.events().isEmpty() || !backend.todayEvents().isEmpty()) {
                qCritical() << "Live native profile contains synthetic or unexpected data"; app.exit(5); return;
            }
            if (!emptyLiveStable.isValid()) {
                emptyLiveInitialRevision = backend.diagnostics().value("revision").toLongLong();
                emptyLiveDensity = backend.preferences().value("density").toString() == "compact" ? "comfortable" : "compact";
                backend.setPreferences({{"density", emptyLiveDensity}});
                emptyLiveStable.start(); return;
            }
            if (emptyLiveStable.elapsed() < 2500 || backend.preferencesPending()
                || backend.preferences().value("density").toString() != emptyLiveDensity
                || backend.diagnostics().value("revision").toLongLong() <= emptyLiveInitialRevision) return;
            emptyLiveCheck.stop(); qInfo() << "Native empty live profile verified through preference save and repeated idle polls: no accounts, conversations or calendar events; fixture revision 0";
            backend.shutdown();
        });
        emptyLiveCheck.start();
        QTimer::singleShot(30000, &app, [&] { if (emptyLiveCheck.isActive()) { qCritical() << "Live profile verification timed out"; app.exit(5); } });
    }
    bool captured = false;
    QTimer captureTimer;
    captureTimer.setInterval(100);
    int settledTicks = 0;
    bool screenshotReplyOpened = false;
    bool screenshotFooterPositioned = false;
    bool screenshotStaged = false;
    QObject::connect(&captureTimer, &QTimer::timeout, &app, [&] {
        if (!backend.connected() || backend.loading() || backend.draft().isEmpty()
            || !backend.property("calendarReady").toBool() || !backend.property("timelineReady").toBool()) { settledTicks = 0; return; }
        const auto captureRoot = engine.rootObjects().value(0);
        const auto splash = captureRoot ? captureRoot->findChild<QQuickItem *>("startupOverlay") : nullptr;
        if (splash && splash->isVisible()) { settledTicks = 0; return; }
        if (parser.isSet("screenshot-reply") && !screenshotReplyOpened) {
            auto root = engine.rootObjects().value(0);
            auto mail = root ? root->findChild<QObject *>("mailWorkspace") : nullptr;
            if (!mail || !QMetaObject::invokeMethod(mail, "openReply")) { QCoreApplication::exit(2); return; }
            screenshotReplyOpened = true; settledTicks = 0; return;
        }
        if (++settledTicks < 3) return;
        if (!screenshotStaged && (parser.isSet("screenshot-set") || parser.isSet("screenshot-invoke") || parser.isSet("screenshot-click"))) {
            // Visual-check staging only: named items and their existing QML
            // functions, applied once after the data has settled.
            screenshotStaged = true; settledTicks = 0;
            const auto root = engine.rootObjects().value(0);
            const auto named = [&](const QString &name) -> QObject * {
                if (name == "backend") return &backend;
                if (!root) return nullptr;
                return root->objectName() == name ? root : root->findChild<QObject *>(name);
            };
            for (const auto &assignment : parser.values("screenshot-set")) {
                const auto dot = assignment.indexOf('.'), equals = assignment.indexOf('=');
                auto target = dot > 0 ? named(assignment.left(dot)) : nullptr;
                const auto value = QJsonDocument::fromJson(("[" + assignment.mid(equals + 1) + "]").toUtf8()).array().at(0).toVariant();
                if (!target || equals < dot || !target->setProperty(assignment.mid(dot + 1, equals - dot - 1).toUtf8(), value)) {
                    qCritical().noquote() << "Screenshot assignment failed:" << assignment; QCoreApplication::exit(2); return;
                }
            }
            for (const auto &call : parser.values("screenshot-invoke")) {
                const auto dot = call.indexOf('.');
                auto target = dot > 0 ? named(call.left(dot)) : nullptr;
                if (!target || !QMetaObject::invokeMethod(target, call.mid(dot + 1).toUtf8())) {
                    qCritical().noquote() << "Screenshot call failed:" << call; QCoreApplication::exit(2); return;
                }
            }
            const auto window = qobject_cast<QQuickWindow *>(root);
            for (const auto &name : parser.values("screenshot-click")) {
                // Delegates are found through the visual tree, not QObject children.
                QQuickItem *target = nullptr;
                std::function<void(QQuickItem *)> find = [&](QQuickItem *item) {
                    if (target || !item || !item->isVisible()) return;
                    if (item->objectName() == name) { target = item; return; }
                    for (auto child : item->childItems()) find(child);
                };
                if (window) find(window->contentItem());
                if (!window || !target) { qCritical().noquote() << "Screenshot click failed:" << name; QCoreApplication::exit(2); return; }
                const auto point = target->mapToScene(QPointF(target->width() / 2, target->height() / 2));
                QMouseEvent press(QEvent::MouseButtonPress, point, window->mapToGlobal(point), Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
                QMouseEvent release(QEvent::MouseButtonRelease, point, window->mapToGlobal(point), Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
                QCoreApplication::sendEvent(window, &press); QCoreApplication::sendEvent(window, &release);
            }
            return;
        }
        if (parser.isSet("screenshot-reply-bottom") && !screenshotFooterPositioned) {
            const auto root = engine.rootObjects().value(0);
            const auto reader = root ? root->findChild<QQuickItem *>("reader") : nullptr;
            if (!reader) { QCoreApplication::exit(2); return; }
            reader->setProperty("contentY", qMax(0.0, reader->property("contentHeight").toDouble() - reader->height()));
            screenshotFooterPositioned = true; settledTicks = 0; return;
        }
        captureTimer.stop();
        if (parser.isSet("self-test-startup-workspace")) {
            const auto root = engine.rootObjects().value(0);
            const auto expected = parser.value("self-test-startup-workspace");
            if (!root || root->property("workspace").toString() != expected || root->property("settingsOpen").toBool()) {
                qCritical() << "Saved initial workspace did not load in the fresh native process:" << expected;
                QCoreApplication::exit(5); return;
            }
            qInfo() << "Fresh native process verified saved initial workspace:" << expected << "theme:" << backend.preferences()["theme"];
        }
        if (parser.isSet("self-test-calendar-restart")) {
            QString error;
            if (!GuiCalendarSelfTest::verifyRestartProjection(backend,
                    qobject_cast<QQuickWindow *>(engine.rootObjects().value(0)),
                    parser.value("self-test-calendar-restart"), error)) {
                qCritical().noquote() << "Restarted native calendar verification failed:" << error;
                QCoreApplication::exit(5); return;
            }
        }
        if (parser.isSet("self-test-composition-restart")) {
            QString error;
            if (!GuiCompositionSelfTest::verifyRestartProjection(backend,
                    qobject_cast<QQuickWindow *>(engine.rootObjects().value(0)),
                    parser.value("self-test-composition-restart"), error)) {
                qCritical().noquote() << "Restarted native draft verification failed:" << error;
                QCoreApplication::exit(5); return;
            }
        }
        if (parser.isSet("screenshot")) {
            auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().value(0));
            if (!window || !window->grabWindow().save(parser.value("screenshot"))) { QCoreApplication::exit(2); return; }
            // Named item geometry beside the image, for layout comparisons.
            QJsonArray items;
            std::function<void(QQuickItem *)> collect = [&](QQuickItem *item) {
                if (!item->isVisible()) return;
                if (!item->objectName().isEmpty()) {
                    const auto r = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
                    items.append(QJsonObject{{"name", item->objectName()}, {"x", r.x()}, {"y", r.y()}, {"w", r.width()}, {"h", r.height()}});
                }
                for (auto child : item->childItems()) collect(child);
            };
            auto top = window->contentItem();
            while (top->parentItem()) top = top->parentItem();
            collect(top);
            QFile geometry(parser.value("screenshot") + ".items.json");
            if (geometry.open(QIODevice::WriteOnly)) geometry.write(QJsonDocument(items).toJson(QJsonDocument::Indented));
        }
        if (parser.isSet("smoke")) backend.shutdown();
    });
    QObject::connect(&backend, &MailBackend::ready, &app, [&] {
        if (captured) return;
        captured = true;
        if (parser.isSet("self-test-calendar-restart")) {
            backend.setCalendarView("day");
            backend.setCalendarDate("2026-09-11");
        }
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
    if (!parser.isSet("self-test-startup-workspace") && (parser.isSet("smoke") || parser.isSet("self-test") || parser.isSet("screenshot") || benchmarkCount || snapshotTest || actionsTest || calendarTest || lifecycleTest) && !engine.rootObjects().isEmpty())
        engine.rootObjects().first()->setProperty("suppressStartupPreference", true);
    if (parser.isSet("self-test-calendar-restart") && !engine.rootObjects().isEmpty()) {
        auto root = engine.rootObjects().first();
        root->setProperty("workspace", "calendar");
        root->setProperty("settingsOpen", false);
    }
    if (parser.isSet("screenshot") && !engine.rootObjects().isEmpty()) {
        auto root = engine.rootObjects().first();
        root->setProperty("workspace", captureView == "calendar" ? "calendar" : "mail");
        root->setProperty("settingsOpen", captureView == "settings");
        root->setProperty("width", qBound(1000, parser.value("screenshot-width").toInt(), 3840));
        root->setProperty("height", qBound(650, parser.value("screenshot-height").toInt(), 2160));
        if (auto settings = root->findChild<QObject *>("settingsView")) settings->setProperty("page", parser.value("screenshot-settings-page"));
    }
    std::unique_ptr<GuiSelfTest> selfTest;
    std::unique_ptr<GuiGoogleDraftSnapshotSelfTest> snapshotSelfTest;
    std::unique_ptr<GuiGoogleDraftActionsSelfTest> actionsSelfTest;
    std::unique_ptr<GuiGoogleCalendarSelfTest> calendarSelfTest;
    std::unique_ptr<GuiAccountLifecycleSelfTest> lifecycleSelfTest;
    QVariantMap attachmentExpected;
    if (parser.isSet("benchmark-search-output") && !engine.rootObjects().isEmpty()) {
        auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        if (!window) return 7;
        searchBenchmark = std::make_unique<GuiSearchBenchmark>(backend, window, parser.value("benchmark-search-output"), &app);
    }
    if (parser.isSet("benchmark-navigation-output") && !engine.rootObjects().isEmpty()) {
        auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        if (!window) return 7;
        interactionBenchmark = std::make_unique<GuiInteractionBenchmark>(backend, window, parser.value("benchmark-navigation-output"), &app);
    }
    if (parser.isSet("benchmark-resize-output") && !engine.rootObjects().isEmpty()) {
        auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        if (!window) return 7;
        resizeBenchmark = std::make_unique<GuiResizeBenchmark>(backend, window, parser.value("benchmark-resize-output"), &app);
    }
    if (parser.isSet("benchmark-calendar-output") && !engine.rootObjects().isEmpty()) {
        auto window = qobject_cast<QQuickWindow *>(engine.rootObjects().first());
        if (!window) return 7;
        calendarBenchmark = std::make_unique<GuiCalendarBenchmark>(backend, window, parser.value("benchmark-calendar-output"), &app);
    }
    if (parser.isSet("self-test") && !engine.rootObjects().isEmpty()) {
        if (parser.isSet("self-test-attachments")) {
            auto window=qobject_cast<QQuickWindow *>(engine.rootObjects().first());
            if (!window) return 5;
            new GuiCompositionSelfTest(backend,window,profile,program,[&](const QVariantMap &expected,const QString &error) {
                if (!error.isEmpty()) { qCritical().noquote()<<"Native attachment integration failed:"<<error; app.exit(5); return; }
                attachmentExpected=expected; backend.setProperty("selfTestCompleted",true); backend.shutdown();
            },&app,true);
        } else selfTest = std::make_unique<GuiSelfTest>(backend, engine.rootObjects().first(), profile, program, &app);
        QTimer::singleShot(parser.isSet("self-test-attachments")?80000:120000, &app, [] { QCoreApplication::exit(6); });
    }
    if (snapshotTest && !engine.rootObjects().isEmpty()) {
        snapshotSelfTest = std::make_unique<GuiGoogleDraftSnapshotSelfTest>(backend,
            qobject_cast<QQuickWindow *>(engine.rootObjects().first()),
            parser.value("google-drafts-expectation"), parser.value("google-drafts-self-test"), &app);
    }
    if (actionsTest && !engine.rootObjects().isEmpty()) {
        actionsSelfTest = std::make_unique<GuiGoogleDraftActionsSelfTest>(backend,
            qobject_cast<QQuickWindow *>(engine.rootObjects().first()),
            parser.value("google-draft-actions-expectation"), parser.value("google-draft-actions-self-test"), &app);
    }
    if (calendarTest && !engine.rootObjects().isEmpty()) {
        calendarSelfTest = std::make_unique<GuiGoogleCalendarSelfTest>(backend,
            qobject_cast<QQuickWindow *>(engine.rootObjects().first()),
            parser.value("google-calendar-expectation"), parser.value("google-calendar-self-test"), &app);
    }
    if (lifecycleTest && !engine.rootObjects().isEmpty()) {
        lifecycleSelfTest = std::make_unique<GuiAccountLifecycleSelfTest>(backend,
            qobject_cast<QQuickWindow *>(engine.rootObjects().first()),
            parser.value("account-lifecycle-expectation"), parser.value("account-lifecycle-self-test"), &app);
    }
    QTimer::singleShot(0, &backend, &MailBackend::start);
    const auto result = app.exec();
    if (searchBenchmark) searchBenchmark->stopForShutdown();
    if (interactionBenchmark) interactionBenchmark->stopForShutdown();
    if (resizeBenchmark) resizeBenchmark->stopForShutdown();
    if (calendarBenchmark) calendarBenchmark->stopForShutdown();
    const bool selfTestCompleted = backend.property("selfTestCompleted").toBool();
    const bool restartVerified = result != 0 || !selfTest || !selfTestCompleted || selfTest->verifyRestart();
    if (result==0 && parser.isSet("self-test-attachments") && selfTestCompleted) {
        for (int i=0;i<60 && QFile::exists(QDir(profile).filePath("engine.json"));++i) QThread::msleep(50);
        QString error;
        if (QFile::exists(QDir(profile).filePath("engine.json"))
            || !GuiCompositionSelfTest::verifyAttachments(profile,program,attachmentExpected,error)) {
            qCritical().noquote()<<"Native attachment restart verification failed:"<<error;
            return 5;
        }
    }
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
    if (result == 0 && parser.isSet("self-test-live-empty") && emptyLiveCheck.isActive()) {
        qCritical() << "Live profile verification exited before its final checks.";
        return 5;
    }
    if (!restartVerified) return 5;
    if (result == 0 && actionsTest && (!actionsSelfTest || !actionsSelfTest->passed())) return 5;
    if (result == 0 && calendarTest && (!calendarSelfTest || !calendarSelfTest->passed())) return 5;
    if (result == 0 && lifecycleTest && (!lifecycleSelfTest || !lifecycleSelfTest->passed())) return 5;
    if (result == 0 && snapshotTest && (!snapshotSelfTest || !snapshotSelfTest->passed())) return 5;
    if (result == 0 && searchBenchmark && !searchBenchmark->passed()) return 9;
    if (result == 0 && interactionBenchmark && !interactionBenchmark->passed()) return 9;
    if (result == 0 && resizeBenchmark && !resizeBenchmark->passed()) return 9;
    if (result == 0 && calendarBenchmark && !calendarBenchmark->passed()) return 9;
    return result;
}
