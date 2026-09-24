#include "DebugConsole.h"
#include "FmIconItem.h"
#include <QClipboard>
#include <QFile>
#include <QDir>
#include <QGuiApplication>
#include <QFontDatabase>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QQuickStyle>
#include <QWheelEvent>
#include <QTemporaryDir>
#include <QtTest>
#include <thread>
#include <cmath>

class DebugBackendFixture : public QObject {
    Q_OBJECT
signals:
    void debugEvent(const QString &, const QString &, const QString &, const QVariantMap &);
    void errorChanged();
    void connectedChanged();
    void googleSyncChanged();
    void assistanceReply(const QString &, const QString &, const QVariantMap &, bool);
};

class DebugConsoleTest : public QObject {
    Q_OBJECT
private slots:
    void retainedStateAndRedaction() {
        DebugConsole log;
        DebugBackendFixture backend;
        backend.setProperty("account", "private-account@example.test");
        backend.setProperty("query", "private search phrase");
        backend.setProperty("connected", true);
        backend.setProperty("diagnostics", QVariantMap{{"schema_version", 29}, {"token", "private-credential"}});
        backend.setProperty("accounts", QVariantList{QVariantMap{{"id", "private-account@example.test"}, {"email", "private-account@example.test"}, {"name", "Private Name"}}});
        backend.setProperty("googleSync", QVariantMap{{"private-account@example.test", QVariantMap{{"active", true}, {"owned", true},
            {"progress", QVariantMap{{"mail", "retrying"}, {"mail_error", "google_rate_limited"}, {"mail_steps", 12}, {"body", "private message text"}}}}}});
        log.attach(&backend);
        emit backend.debugEvent("ERROR", "IPC", "Rate limited; retry later", {{"method", "google_send"}, {"request_ref", "42"}, {"code", "google_rate_limited"}, {"retry_after_secs", 23}, {"body", "private body"}, {"token", "private-credential"}});
        log.record("WARN", "Qt", "access_token=secret-value Bearer another-secret person@example.test https://example.test/?code=secret 'mail subject' C:\\Users\\Private\\store");
        const auto report = log.report();
        for (const auto secret : {"private-account", "Private Name", "private search", "private-credential", "private message", "private body", "secret-value", "another-secret", "person@example", "code=secret", "mail subject", "Users\\Private"})
            QVERIFY2(!report.contains(secret), secret);
        QVERIFY(report.contains("account-1")); QVERIFY(report.contains("schema_version"));
        QVERIFY(report.contains("google_rate_limited")); QVERIFY(report.contains("retry_after_secs\":23"));
        QVERIFY(report.contains("request_ref\":\"42\""));
        QVERIFY(log.events("ERROR", "google_send", "Mail").contains("request_ref"));
        log.record("INFO", "IPC", "Reconnect", {{"method", "google_auth_start"}});
        QVERIFY(log.events("All", "Reconnect", "Sync").contains("google_auth_start"));
        log.record("INFO", "IPC", "Calendar write", {{"method", "google_calendar_write"}});
        QVERIFY(log.events("All", "google_calendar_write", "Calendar").contains("Calendar write"));
        log.record("INFO", "IPC", "HTML projection", {{"method", "message_html"}});
        QVERIFY(log.events("All", "message_html", "Mail").contains("HTML projection"));
        QVERIFY(log.events("WARN", "[email]", "App").contains("[secret]"));
        QVERIFY(report.contains("mail_steps = 12"));
        emit backend.assistanceReply("private-ticket", "assist_job", {{"job_id", "private-job"}, {"status", "failed"},
            {"error", QVariantMap{{"code", "invalid_response"}, {"message", "Model returned invalid JSON"}}}, {"body", "private body"}}, true);
        QVERIFY(log.events("ERROR").contains("Model returned invalid JSON"));
        QVERIFY(!log.report().contains("private-job"));
        const auto count = log.count(); log.refresh(); log.refresh(); QCOMPARE(log.count(), count);
        backend.setProperty("googleSync", QVariantMap{{"private-account@example.test", QVariantMap{{"active", true}, {"progress", QVariantMap{{"mail", "waiting"}, {"mail_steps", 13}}}}}});
        emit backend.googleSyncChanged();
        QVERIFY(log.events().contains("mail_steps = 13"));
        QVERIFY(log.events("Warnings + errors").contains("google_rate_limited"));
        QVERIFY(!log.events("ERROR", "absent-filter").size());
        log.copyReport(); QVERIFY(QGuiApplication::clipboard()->text().contains("RUNTIME STATE"));
        QVERIFY(QGuiApplication::clipboard()->text().contains("mail_steps = 13"));
    }
    void boundedThreadedQtCapture() {
        DebugConsole log; log.captureQtMessages();
        std::thread worker([] { qWarning("diagnostic worker warning person@example.test"); }); worker.join();
        QVERIFY(log.events("WARN").contains("diagnostic worker warning [email]"));
        for (int i = 0; i < 1100; ++i) log.record("INFO", "test", QString("event %1 ").arg(i) + QString(4000, 'x'));
        QCOMPARE(log.count(), 1000); QVERIFY(log.dropped() >= 102);
        QVERIFY(log.events().size() <= 2050000);
        QVERIFY(!log.events().contains("event 0 "));
        QVERIFY(log.events().contains("event 1099 "));
        log.clear(); QCOMPARE(log.count(), 1); QCOMPARE(log.dropped(), qulonglong(0));
    }
    void diskRotationAndEngineScopes() {
        QTemporaryDir dir; QVERIFY(dir.isValid());
        DebugConsole log; log.setLogDirectory(dir.path());
        for (int i = 0; i < 3000; ++i) log.record("INFO", "sync", QString("step %1 ").arg(i) + QString("word ").repeated(300));
        const auto directory = QDir(log.logDirectory());
        QVERIFY(QFile::exists(directory.filePath("gui.log.1")));
        QVERIFY(QFile(directory.filePath("gui.log")).size() <= 2560 * 1024);
        QVERIFY(QFile(directory.filePath("gui.log.1")).size() <= 2560 * 1024);
        QFile engine(directory.filePath("engine.log")); QVERIFY(engine.open(QIODevice::WriteOnly));
        engine.write("{\"at\":\"2026-09-24T00:00:00.000Z\",\"level\":\"INFO\",\"scope\":\"Model\",\"event\":\"completed\",\"fields\":{\"summary_chars\":53,\"sender_kind\":\"human\",\"category_ids\":[\"work\"]}}\n");
        engine.close(); log.refresh();
        QVERIFY(log.events("All", "summary_chars", "Model").contains("\"summary_chars\":53"));
        QVERIFY(!log.events("All", "summary_chars", "Sync").contains("summary_chars"));
        QTest::qWait(15);
        QVERIFY(QFile::rename(directory.filePath("engine.log"), directory.filePath("engine.log.1")));
        QVERIFY(engine.open(QIODevice::WriteOnly));
        engine.write("{\"at\":\"2026-09-24T00:00:01.000Z\",\"level\":\"WARN\",\"scope\":\"Sync\",\"event\":\"mail_history_expired\",\"fields\":{\"generation\":2}}\n");
        engine.close(); log.refresh();
        QVERIFY(log.events("WARN", "mail_history_expired", "Sync").contains("\"generation\":2"));
    }
    void newProfileWaitsForEngineAdmission() {
        QTemporaryDir dir; QVERIFY(dir.isValid());
        const auto profile = dir.filePath("new-profile");
        DebugConsole log;
        DebugBackendFixture backend;
        log.attach(&backend);
        log.setLogDirectory(profile);
        QVERIFY(!QFile::exists(profile));
        backend.setProperty("connected", true);
        emit backend.connectedChanged();
        QVERIFY(QFile::exists(QDir(profile).filePath("diagnostics/gui.log")));
    }
    void keyboardAndProductionPanel() {
        DebugConsole log;
        log.record("WARN", "sync", "account-1 mail = retrying; google_rate_limited", {{"retry_after_secs", 23}});
        log.refresh();
        QTemporaryDir dir; QVERIFY(dir.isValid());
        const auto modulePath = dir.filePath("ConsoleTest"); QVERIFY(QDir().mkpath(modulePath));
        for (const auto name : {"Theme.qml", "DebugConsole.qml", "FmButton.qml", "FmTextField.qml", "FmComboBox.qml", "FmScrollBar.qml", "FmIcon.qml", "FmToolTip.qml"})
            QVERIFY2(QFile::copy(QString(FM_GUI_SOURCE_DIR) + '/' + name, modulePath + '/' + name), name);
        QFile module(modulePath + "/qmldir"); QVERIFY(module.open(QIODevice::WriteOnly));
        module.write("module ConsoleTest\nsingleton Theme 1.0 Theme.qml\nDebugConsole 1.0 DebugConsole.qml\n"); module.close();
        QQmlApplicationEngine qml; FmIconItem::registerType();
        QStringList warnings;
        connect(&qml, &QQmlEngine::warnings, this, [&](const QList<QQmlError> &items) { for (const auto &item : items) warnings.append(item.toString()); });
        qml.addImportPath(dir.path()); qml.rootContext()->setContextProperty("testDiagnostics", &log);
        qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import ConsoleTest
ApplicationWindow {
    id: window; width: 1280; height: 820; visible: true
    property bool searching: false
    property string testTheme: "graphite"
    Binding { target: Theme; property: "themeId"; value: window.testTheme }
    TextField { objectName: "editor"; width: 300; text: "draft words" }
    Item { objectName: "idle"; focus: true }
    DebugConsole { hostWindow: window; diagnostics: testDiagnostics; searchActive: window.searching }
})QML", QUrl::fromLocalFile(dir.filePath("Harness.qml")));
        QVERIFY2(!qml.rootObjects().isEmpty(), qPrintable(warnings.join('\n')));
        auto window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); QVERIFY(window);
        auto console = window->findChild<QObject *>("debugConsole"); QVERIFY(console);
        auto editor = window->findChild<QQuickItem *>("editor"); QVERIFY(editor);
        auto idle = window->findChild<QQuickItem *>("idle"); QVERIFY(idle);
        window->requestActivate(); QTest::qWait(100);
        editor->forceActiveFocus(); QTest::keyClick(window, Qt::Key_QuoteLeft);
        QVERIFY(!console->property("opened").toBool()); QVERIFY(editor->property("text").toString().contains('`'));
        QTest::keyClick(window, Qt::Key_AsciiTilde, Qt::ShiftModifier);
        QVERIFY(!console->property("opened").toBool()); QVERIFY(editor->property("text").toString().contains('~'));
        idle->forceActiveFocus(); window->setProperty("searching", true);
        QTest::keyClick(window, Qt::Key_QuoteLeft); QVERIFY(!console->property("opened").toBool());
        window->setProperty("searching", false);
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTRY_VERIFY(console->property("opened").toBool());
        QVERIFY(console->property("displayText").toString().contains("google_rate_limited"));
        QCOMPARE(console->property("x").toDouble(), 32.0);
        QCOMPARE(console->property("y").toDouble(), 32.0);
        QCOMPARE(console->property("width").toDouble(), 1216.0);
        console->setProperty("selectedScope", "Sync");
        QVERIFY(console->property("displayText").toString().contains("google_rate_limited"));
        console->setProperty("selectedScope", "Model");
        QVERIFY(!console->property("displayText").toString().contains("google_rate_limited"));
        console->setProperty("selectedScope", "All");
        auto filter = window->findChild<QQuickItem *>("debugFilter"); QVERIFY(filter);
        filter->setProperty("text", "absent-filter"); QVERIFY(console->property("displayText").toString().isEmpty());
        filter->forceActiveFocus(); QTest::keyClick(window, Qt::Key_QuoteLeft);
        QVERIFY(console->property("opened").toBool()); QVERIFY(filter->property("text").toString().contains('`'));
        filter->setProperty("text", "");
        auto copy = window->findChild<QQuickItem *>("debugCopy"); QVERIFY(copy);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, copy->mapToScene(QPointF(copy->width()/2, copy->height()/2)).toPoint());
        QVERIFY(QGuiApplication::clipboard()->text().contains("google_rate_limited"));
        console->setProperty("showState", true); QVERIFY(console->property("displayText").toString().contains("architecture"));
        console->setProperty("showState", false);
        auto events = window->findChild<QQuickItem *>("debugEvents"); QVERIFY(events); events->forceActiveFocus();
        console->setProperty("paused", true); const auto frozen = console->property("displayText");
        log.record("ERROR", "test", "new error while view paused"); log.refresh();
        QCOMPARE(console->property("displayText"), frozen);
        log.copyReport(); QVERIFY(QGuiApplication::clipboard()->text().contains("new error while view paused"));
        console->setProperty("paused", false); QVERIFY(console->property("displayText").toString().contains("new error while view paused"));
        for (int i = 0; i < 1200; ++i) log.record("INFO", "sync", QString("scroll event %1").arg(i));
        log.refresh();
        QTRY_VERIFY(events->property("atYEnd").toBool());
        // Native wheel input stops following before the next diagnostics refresh.
        const auto point = events->mapToScene(QPointF(400, 200));
        QWheelEvent wheel(point, window->mapToGlobal(point.toPoint()), {}, QPoint(0, 120), Qt::NoButton, Qt::NoModifier, Qt::NoScrollPhase, false);
        QCoreApplication::sendEvent(window, &wheel);
        QTRY_VERIFY(!console->property("followTail").toBool());
        QTest::qWait(200);
        QVERIFY(!events->property("atYEnd").toBool());
        auto anchor = [&]() -> QPair<QString, qreal> {
            const auto y = events->property("contentY").toDouble();
            auto content = events->property("contentItem").value<QQuickItem *>();
            if (content) for (auto child : content->childItems())
                if (child->property("sequence").isValid() && child->y() <= y + 1 && child->y() + child->height() > y + 1)
                    return {child->property("sequence").toString(), y - child->y()};
            return {};
        };
        const auto reading = anchor(); QVERIFY(!reading.first.isEmpty());
        for (int batch = 0; batch < 8; ++batch) {
            for (int i = 0; i < 7; ++i) log.record("INFO", "sync", QString("new batch %1 event %2").arg(batch).arg(i));
            log.refresh(); QTest::qWait(30);
            QCOMPARE(anchor().first, reading.first);
            QVERIFY(std::abs(anchor().second - reading.second) < 1.0);
            QVERIFY(!console->property("followTail").toBool());
        }
        // Selecting a row exposes complete, selectable metadata without resuming.
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, point.toPoint());
        QVERIFY(!console->property("selectedText").toString().isEmpty());
        QVERIFY(!console->property("followTail").toBool());
        console->setProperty("selectedText", "");
        auto tail = window->findChild<QQuickItem *>("debugTail"); QVERIFY(tail);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, tail->mapToScene(QPointF(tail->width()/2, tail->height()/2)).toPoint());
        QTRY_VERIFY(events->property("atYEnd").toBool());
        QVERIFY(console->property("followTail").toBool());
        for (int batch = 0; batch < 8; ++batch) {
            log.record("INFO", "local model", "Summary completed", {{"elapsed_ms", batch * 100}});
            log.refresh(); QTest::qWait(30);
            QVERIFY(events->property("atYEnd").toBool());
            QVERIFY(console->property("followTail").toBool());
        }
        window->resize(1000, 650); QTest::qWait(100);
        QVERIFY(events->property("atYEnd").toBool());
        window->resize(1280, 820); QTest::qWait(100);
        QVERIFY(events->property("atYEnd").toBool());
        events->forceActiveFocus(); QTest::keyClick(window, Qt::Key_PageUp);
        QVERIFY(!console->property("followTail").toBool());
        QTest::keyClick(window, Qt::Key_End); QTRY_VERIFY(events->property("atYEnd").toBool());
        auto bar = events->findChild<QQuickItem *>("persistentScrollBar"); QVERIFY(bar);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, bar->mapToScene(QPointF(bar->width()/2, bar->height()/2)).toPoint());
        QTRY_VERIFY(!console->property("followTail").toBool());
        const auto dragged = anchor(); QVERIFY(!dragged.first.isEmpty());
        log.record("WARN", "sync", "Retain scrollbar position"); log.refresh(); QTest::qWait(100);
        QCOMPARE(anchor().first, dragged.first);
        QVERIFY(std::abs(anchor().second - dragged.second) < 1.0);
        events->forceActiveFocus(); QTest::keyClick(window, Qt::Key_Home);
        QTRY_VERIFY(events->property("atYBeginning").toBool());
        for (int i = 0; i < 1100; ++i) log.record("INFO", "sync", QString("replacement event %1").arg(i));
        log.refresh(); QTest::qWait(100);
        QVERIFY(events->property("atYBeginning").toBool());
        QVERIFY(!console->property("followTail").toBool());
        QTest::keyClick(window, Qt::Key_End); QTRY_VERIFY(events->property("atYEnd").toBool());
        QTest::keyClick(window, Qt::Key_Escape); QTRY_VERIFY(!console->property("opened").toBool());
        QTRY_VERIFY(idle->hasActiveFocus());
        QTest::keyClick(window, Qt::Key_AsciiTilde, Qt::ShiftModifier); QTRY_VERIFY(console->property("opened").toBool());
        QTRY_VERIFY(events->property("atYEnd").toBool());
        for (const auto theme : {"graphite", "paper", "harbour", "terminal"}) {
            window->setProperty("testTheme", theme); QTest::qWait(30);
            if (qEnvironmentVariableIsSet("FM_DEBUG_CAPTURE_DIR")) {
                const auto file = QDir(qEnvironmentVariable("FM_DEBUG_CAPTURE_DIR")).filePath(QString(theme) + ".png");
                QVERIFY(window->grabWindow().save(file));
            }
        }
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTRY_VERIFY(!console->property("opened").toBool());
        QVERIFY2(warnings.isEmpty(), qPrintable(warnings.join('\n')));
    }
};
int main(int argc, char **argv) {
    QGuiApplication app(argc, argv); QQuickStyle::setStyle("Basic");
    for (const auto weight : {"Regular", "Medium", "SemiBold"}) QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf");
    DebugConsoleTest test; return QTest::qExec(&test, argc, argv);
}
#include "DebugConsoleTest.moc"
