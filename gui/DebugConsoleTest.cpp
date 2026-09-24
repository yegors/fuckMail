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
#include <QTemporaryDir>
#include <QtTest>
#include <thread>

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
        emit backend.debugEvent("ERROR", "IPC", "Rate limited; retry later", {{"method", "google_send"}, {"code", "google_rate_limited"}, {"retry_after_secs", 23}, {"body", "private body"}, {"token", "private-credential"}});
        log.record("WARN", "Qt", "access_token=secret-value Bearer another-secret person@example.test https://example.test/?code=secret 'mail subject' C:\\Users\\Private\\store");
        const auto report = log.report();
        for (const auto secret : {"private-account", "Private Name", "private search", "private-credential", "private message", "private body", "secret-value", "another-secret", "person@example", "code=secret", "mail subject", "Users\\Private"})
            QVERIFY2(!report.contains(secret), secret);
        QVERIFY(report.contains("account-1")); QVERIFY(report.contains("schema_version"));
        QVERIFY(report.contains("google_rate_limited")); QVERIFY(report.contains("retry_after_secs\":23"));
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
    id: window; width: 1000; height: 650; visible: true
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
        auto output = window->findChild<QQuickItem *>("debugOutput"); QVERIFY(output); output->forceActiveFocus();
        console->setProperty("paused", true); const auto frozen = console->property("displayText");
        log.record("ERROR", "test", "new error while view paused"); log.refresh();
        QCOMPARE(console->property("displayText"), frozen);
        log.copyReport(); QVERIFY(QGuiApplication::clipboard()->text().contains("new error while view paused"));
        console->setProperty("paused", false); QVERIFY(console->property("displayText").toString().contains("new error while view paused"));
        QTest::keyClick(window, Qt::Key_Escape); QTRY_VERIFY(!console->property("opened").toBool());
        QTRY_VERIFY(idle->hasActiveFocus());
        QTest::keyClick(window, Qt::Key_AsciiTilde, Qt::ShiftModifier); QTRY_VERIFY(console->property("opened").toBool());
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
