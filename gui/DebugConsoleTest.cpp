// QtTest suite for DebugConsole: levels, facilities and redaction, threaded
// capture limits, log rotation and engine.log, console commands and export,
// running `fm`, and the production DebugConsole.qml panel's keyboard handling.
#include "DebugConsole.h"
#include "FmIconItem.h"
#include <QClipboard>
#include <QFile>
#include <QDir>
#include <QGuiApplication>
#include <QFontDatabase>
#include <QJsonDocument>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QQuickStyle>
#include <QStyleHints>
#include <QWheelEvent>
#include <QTemporaryDir>
#include <QtTest>
#include <thread>

class DebugBackendFixture : public QObject {
    Q_OBJECT
public:
    QStringList synced;
    QStringList assistance;
    Q_INVOKABLE void refreshGoogleSync(const QString &account) { synced.append("refresh:" + account); }
    Q_INVOKABLE void startGoogleSync(const QString &account, const QVariantMap &) { synced.append("start:" + account); }
    Q_INVOKABLE QString assistanceRequest(const QString &method, const QVariantMap &, const QString & = {}) { assistance.append(method); return "t" + QString::number(assistance.size()); }
signals:
    void debugEvent(const QString &, const QString &, const QString &, const QVariantMap &);
    void errorChanged();
    void connectedChanged();
    void googleSyncChanged();
    void assistanceReply(const QString &, const QString &, const QVariantMap &, bool);
};

namespace {
void settle() { QCoreApplication::processEvents(); QCoreApplication::processEvents(); }
void typeText(QWindow *window, const char *text) { for (auto c = text; *c; ++c) QTest::keyClick(window, *c); }
QQuickItem *findItem(QQuickItem *root, const QString &name) {
    if (!root) return nullptr;
    if (root->objectName() == name) return root;
    for (auto child : root->childItems()) if (auto match = findItem(child, name)) return match;
    return nullptr;
}
QString text(const DebugConsole &log) { return log.exportLines().join('\n'); }
QString rows(DebugConsole &log) {
    settle();
    auto model = qobject_cast<QAbstractItemModel *>(log.lines());
    QStringList out;
    for (int i = 0; i < model->rowCount(); ++i) out.append(model->data(model->index(i, 0), DebugLogModel::MessageRole).toString());
    return out.join('\n');
}
}

class DebugConsoleTest : public QObject {
    Q_OBJECT
private slots:
    void levelsFacilitiesAndRedaction() {
        DebugConsole log;
        DebugBackendFixture backend;
        backend.setProperty("account", "private-account@example.test");
        backend.setProperty("connected", true);
        backend.setProperty("googleSync", QVariantMap{{"private-account@example.test", QVariantMap{{"active", true}, {"owned", true},
            {"progress", QVariantMap{{"mail", "retrying"}, {"mail_error", "google_rate_limited"}, {"mail_steps", 12}, {"body", "private message text"}}}}}});
        log.attach(&backend);
        emit backend.debugEvent("ERROR", "IPC", "Rate limited; retry later", {{"method", "google_send"}, {"request_ref", "42"}, {"code", "google_rate_limited"}, {"retry_after_secs", 23}, {"body", "private body"}, {"token", "private-credential"}});
        log.record("WARN", "Qt", "access_token=secret-value Bearer another-secret person@example.test https://example.test/?code=secret 'mail subject' C:\\Users\\Private\\store");
        log.record("info", "IPC", "Calendar write", {{"method", "google_calendar_write"}});
        log.record("info", "IPC", "Reconnect", {{"method", "google_auth_start"}});
        log.record("info", "IPC", "HTML projection", {{"method", "message_html"}});
        log.record("debug", "IPC", "Search ran", {{"method", "search_senders"}});
        log.record("info", "IPC", "Payload", {{"method", "google_sent"}, {"json", QVariantMap{{"error", QVariantMap{{"code", 503}, {"from", "someone@example.test"}, {"retry", true}}}, {"items", QVariantList{1, 2}}}}});
        emit backend.assistanceReply("ticket", "assist_job", {{"job_id", "private-job"}, {"status", "failed"},
            {"error", QVariantMap{{"code", "invalid_response"}, {"message", "Model returned invalid JSON"}}}}, true);
        log.setLevel(7);
        const auto all = text(log);
        for (const auto secret : {"private-account", "private-credential", "private message", "private body", "secret-value", "another-secret", "person@example", "someone@example", "code=secret", "mail subject", "Users\\Private", "private-job"})
            QVERIFY2(!all.contains(secret), secret);
        QVERIFY(all.contains("<3> err     gmail   Rate limited; retry later  {code=google_rate_limited method=google_send request_ref=42 retry_after_secs=23}"));
        QVERIFY(all.contains("<4> warning app     [secret]"));
        QVERIFY(all.contains("cal     Calendar write"));
        QVERIFY(all.contains("sync    Reconnect"));
        QVERIFY(all.contains("net     HTML projection"));
        QVERIFY(all.contains("<7> debug   db      Search ran"));
        QVERIFY(all.contains("err     llm     Job failed Model returned invalid JSON"));
        QVERIFY(all.contains("account-1 mail_steps = 12"));
        // JSON goes under its line, pretty-printed and indented four spaces; strings are scrubbed.
        QVERIFY(all.contains("\n            \"code\": 503,"));
        QVERIFY(all.contains("\"from\": \"[email]\""));
        // Filters: level shows that level and worse; facility; grep also searches details and JSON.
        log.setLevel(3);
        QVERIFY(text(log).contains("Rate limited")); QVERIFY(!text(log).contains("[secret]")); QVERIFY(!text(log).contains("Calendar write"));
        log.setLevel(6);
        QVERIFY(!text(log).contains("Search ran"));
        log.setFacility("cal"); QVERIFY(text(log).contains("Calendar write")); QVERIFY(!text(log).contains("Reconnect"));
        log.setFacility("bogus"); QCOMPARE(log.facility(), QString("all"));
        log.setGrep("REQUEST_REF=42"); QVERIFY(text(log).contains("Rate limited")); QVERIFY(!text(log).contains("Reconnect"));
        log.setGrep("503"); QVERIFY(text(log).contains("Payload")); QVERIFY(!text(log).contains("Rate limited"));
        log.setGrep("");
        const auto count = log.count(); log.refresh(); log.refresh(); QCOMPARE(log.count(), count);
        backend.setProperty("googleSync", QVariantMap{{"private-account@example.test", QVariantMap{{"active", true}, {"progress", QVariantMap{{"mail", "waiting"}, {"mail_steps", 13}}}}}});
        emit backend.googleSyncChanged();
        QVERIFY(text(log).contains("mail_steps = 13"));
        settle(); log.refresh();
        QVERIFY(log.errors() >= 2); QVERIFY(log.warnings() >= 1);
    }
    void boundedThreadedCaptureClearAndPause() {
        QTemporaryDir dir; QVERIFY(dir.isValid());
        DebugConsole log; log.captureQtMessages(); log.setLogDirectory(dir.path());
        std::thread worker([] { qWarning("diagnostic worker warning person@example.test"); }); worker.join();
        settle();
        QVERIFY(text(log).contains("warning app     diagnostic worker warning [email]"));
        for (int i = 0; i < 5100; ++i) log.record("info", "sync", QString("event %1 ").arg(i) + QString(200, 'x'));
        settle();
        QCOMPARE(log.count(), DebugConsole::retained);
        QCOMPARE(log.lines()->property("count").isValid(), false);
        auto model = qobject_cast<QAbstractItemModel *>(log.lines());
        QCOMPARE(model->rowCount(), DebugConsole::shown);
        QVERIFY(!text(log).contains("event 0 "));
        QVERIFY(rows(log).contains("event 5099 ")); QVERIFY(!rows(log).contains("event 4000 "));
        QCOMPARE(log.matched(), DebugConsole::retained);
        // Clear hides the view only; the file keeps everything.
        log.clear(); settle();
        QCOMPARE(model->rowCount(), 0); QCOMPARE(log.count(), DebugConsole::retained); QCOMPARE(log.matched(), 0);
        log.record("info", "sync", "after clear"); settle();
        QCOMPARE(model->rowCount(), 1);
        // Pause stops capturing in the console; the log file keeps recording.
        log.setPaused(true);
        log.record("err", "sync", "while paused"); settle();
        QVERIFY(!rows(log).contains("while paused")); QVERIFY(!text(log).contains("while paused"));
        log.setPaused(false);
        log.record("info", "sync", "after resume"); settle();
        QVERIFY(rows(log).contains("after resume"));
        QFile file(QDir(log.logDirectory()).filePath("gui.log")); QVERIFY(file.open(QIODevice::ReadOnly));
        const auto disk = QString::fromUtf8(file.readAll());
        QVERIFY(disk.contains("while paused")); QVERIFY(disk.contains("event 5099 ")); QVERIFY(disk.contains(" <3> err     sync    while paused"));
    }
    void diskRotationAndEngineFacilities() {
        QTemporaryDir dir; QVERIFY(dir.isValid());
        DebugConsole log; log.setLogDirectory(dir.path());
        for (int i = 0; i < 3000; ++i) log.record("info", "sync", QString("step %1 ").arg(i) + QString("word ").repeated(300));
        const auto directory = QDir(log.logDirectory());
        QVERIFY(QFile::exists(directory.filePath("gui.log.1")));
        QVERIFY(QFile(directory.filePath("gui.log")).size() <= 2560 * 1024);
        QVERIFY(QFile(directory.filePath("gui.log.1")).size() <= 2560 * 1024);
        QFile engine(directory.filePath("engine.log")); QVERIFY(engine.open(QIODevice::WriteOnly));
        engine.write("{\"at\":\"2026-09-24T00:00:00.000Z\",\"level\":\"INFO\",\"scope\":\"Model\",\"event\":\"completed\",\"fields\":{\"summary_chars\":53,\"sender_kind\":\"human\",\"category_ids\":[\"work\"]}}\n");
        engine.close(); log.refresh();
        log.setFacility("llm");
        QVERIFY(text(log).contains("2026-09-24T00:00:00.000Z <6> info    llm     completed  {sender_kind=human summary_chars=53}"));
        QVERIFY(text(log).contains("\"category_ids\""));
        log.setFacility("sync"); QVERIFY(!text(log).contains("summary_chars"));
        QTest::qWait(15);
        QVERIFY(QFile::rename(directory.filePath("engine.log"), directory.filePath("engine.log.1")));
        QVERIFY(engine.open(QIODevice::WriteOnly));
        engine.write("{\"at\":\"2026-09-24T00:00:01.000Z\",\"level\":\"WARN\",\"scope\":\"Sync\",\"event\":\"mail_history_expired\",\"fields\":{\"generation\":2}}\n");
        engine.close(); log.refresh();
        QVERIFY(text(log).contains("<4> warning sync    mail_history_expired  {generation=2}"));
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
    void commandsExportAndState() {
        QTemporaryDir dir; QVERIFY(dir.isValid());
        DebugConsole log;
        DebugBackendFixture backend;
        backend.setProperty("connected", true);
        backend.setProperty("accounts", QVariantList{QVariantMap{{"id", "a1"}, {"email", "yegor@example.test"}, {"name", "Work"}, {"mail_retention", "1_year"}},
            QVariantMap{{"id", "a2"}, {"email", "home@example.test"}, {"name", "Personal"}}});
        backend.setProperty("googleSync", QVariantMap{{"a1", QVariantMap{{"active", true}, {"progress", QVariantMap{{"mail", "running"}}}}}});
        backend.setProperty("diagnostics", QVariantMap{{"schema_version", 42}});
        log.attach(&backend);
        QCOMPARE(log.run("help").value("tail").toBool(), true);
        QVERIFY(rows(log).contains("› help")); QVERIFY(rows(log).contains("grep <text>          filter lines · grep alone clears"));
        log.run("bogus");
        QVERIFY(rows(log).contains("bogus: command not found · type help"));
        log.run("level err"); QCOMPARE(log.level(), 3);
        // Typed commands and replies show whatever the filters.
        log.run("frobnicate"); QVERIFY(rows(log).contains("› frobnicate")); QVERIFY(rows(log).contains("frobnicate: command not found"));
        log.run("level nope"); QCOMPARE(log.level(), 3); QVERIFY(rows(log).contains("unknown level \"nope\""));
        log.run("level info"); QCOMPARE(log.level(), 6);
        log.run("fac gmail"); QCOMPARE(log.facility(), QString("gmail"));
        log.run("fac all"); QCOMPARE(log.facility(), QString("all"));
        log.run("grep Rate limited"); QCOMPARE(log.grep(), QString("Rate limited"));
        log.run("grep"); QCOMPARE(log.grep(), QString());
        log.run("pause"); QVERIFY(log.paused()); log.run("resume"); QVERIFY(!log.paused());
        QCOMPARE(log.run("state").value("tab").toString(), QString("state"));
        QCOMPARE(log.run("logs").value("tab").toString(), QString("logs"));
        QVERIFY(log.run("exit").value("close").toBool()); QVERIFY(log.run("quit").value("close").toBool());
        log.run("sync yegor"); QCOMPARE(backend.synced, QStringList{"refresh:a1"});
        log.run("sync personal"); QCOMPARE(backend.synced, (QStringList{"refresh:a1", "start:a2"}));
        log.run("sync nobody"); QVERIFY(rows(log).contains("no account matches \"nobody\""));
        QVERIFY(rows(log).contains("yegor@example.test: manual sync requested"));
        log.run("clear"); QCOMPARE(qobject_cast<QAbstractItemModel *>(log.lines())->rowCount(), 0);
        log.record("err", "IPC", "Upstream failed", {{"method", "google_sent"}, {"code", "unavailable"}, {"json", QVariantMap{{"error", QVariantMap{{"code", 503}}}}}});
        log.record("debug", "IPC", "hidden at info");
        const auto path = log.exportLog(dir.path());
        QVERIFY(QFileInfo(path).fileName().startsWith("fuckmail-")); QVERIFY(path.endsWith(".log"));
        QFile file(path); QVERIFY(file.open(QIODevice::ReadOnly));
        const auto exported = QString::fromUtf8(file.readAll());
        QVERIFY(exported.contains(QRegularExpression(R"(^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z <3> err     gmail   Upstream failed  \{code=unavailable method=google_sent\}\n    \{\n        "error": \{\n            "code": 503)", QRegularExpression::MultilineOption)));
        QVERIFY(!exported.contains("hidden at info"));
        QVERIFY(rows(log).contains("exported 1 lines → "));
        // The state tab: a read-only tree with live, unaliased account details.
        emit backend.assistanceReply("t", "assist_settings_get", {{"settings", QVariantMap{{"provider", "ollama"}, {"endpoint", "http://127.0.0.1:11434"},
            {"model", QVariantMap{{"name", "qwen2.5:7b"}}}, {"accounts", QVariantMap{{"a1", QVariantMap{{"summaries", false}}}}}}}, {"revision", 3}}, true);
        emit backend.assistanceReply("t", "assist_progress", {{"accounts", QVariantList{QVariantMap{{"account", "a1"}, {"queued", 12}, {"seconds_per_message", 1.5}, {"model_configured", true}}}}}, true);
        log.requestState();
        QCOMPARE(backend.assistance, (QStringList{"assist_settings_get", "assist_progress"}));
        QStringList state;
        for (const auto &row : log.stateRows({{"view", "mail"}, {"theme", "graphite"}})) {
            const auto r = row.toMap();
            state.append(QString(r.value("depth").toInt() * 2, ' ') + r.value("key").toString() + (r.value("kind") == "section" ? "" : " " + r.value("value").toString() + " " + r.value("kind").toString()));
        }
        const auto tree = state.join('\n');
        for (const auto expected : {"app", "  qt ", "  log_lines ", "ui", "  view \"mail\" string", "  theme", "accounts", "  Work", "    address \"yegor@example.test\" string",
                 "    status \"syncing\" string", "    retention \"1_year\" string", "    auto_summarize false bool", "    auto_label true bool", "  Personal", "    status \"paused\" string",
                 "llm", "  provider \"ollama\" string", "  model \"qwen2.5:7b\" string", "  status \"ok\" string", "  queue 12 number", "  avg_latency \"1.50 s\" string",
                 "network", "db", "  schema 42 number"})
            QVERIFY2(tree.contains(expected), expected);
    }
    void fmCommandRunsTheCli() {
#ifndef FM_CLI_PATH
        QSKIP("fm is not part of this build");
#else
        QVERIFY2(QFileInfo(FM_CLI_PATH).isExecutable(), FM_CLI_PATH);
        QTemporaryDir profile; QVERIFY(profile.isValid());
        DebugConsole log;
        log.setLogDirectory(profile.path());
        log.record("info", "sync", "persisted line");
        log.setCliProgram(FM_CLI_PATH);
        QSignalSpy running(&log, &DebugConsole::cliChanged);
        QVERIFY(!log.paused());
        log.run("fm");
        QVERIFY(log.cliRunning());
        // fm pauses live events so its output stays put; its own lines still show.
        QVERIFY(log.paused());
        QVERIFY(rows(log).contains("live log paused so fm output stays put"));
        log.record("info", "sync", "live event while paused");
        QTRY_VERIFY_WITH_TIMEOUT(!log.cliRunning(), 15000);
        QVERIFY(rows(log).contains("› fm"));
        // Bare fm shows the help, not a missing-command error.
        QVERIFY2(rows(log).contains("Usage: fm"), qPrintable(rows(log)));
        QVERIFY(!rows(log).contains("requires a subcommand"));
        QVERIFY(!rows(log).contains("exited with code"));
        QVERIFY(!rows(log).contains("live event while paused"));
        // CLI output is shown and copied, but never written to gui.log.
        QVERIFY(text(log).contains("Usage: fm"));
        QFile disk(QDir(log.logDirectory()).filePath("gui.log")); QVERIFY(disk.open(QIODevice::ReadOnly));
        const auto persisted = QString::fromUtf8(disk.readAll());
        QVERIFY(persisted.contains("persisted line")); QVERIFY(persisted.contains("live event while paused")); QVERIFY(!persisted.contains("Usage: fm"));
        // --json gathers stdout into one expandable line; failures report the exit code.
        log.run("clear");
        log.run("fm --json mail senders");
        QTRY_VERIFY_WITH_TIMEOUT(!log.cliRunning(), 15000);
        auto model = qobject_cast<QAbstractItemModel *>(log.lines());
        QString json;
        for (int i = 0; i < model->rowCount(); ++i)
            if (model->data(model->index(i, 0), DebugLogModel::MessageRole).toString() == "fm --json mail senders")
                json = model->data(model->index(i, 0), DebugLogModel::JsonRole).toString();
        QVERIFY2(json.contains("invalid_request") && json.contains("--account"), qPrintable(rows(log) + "\n" + json));
        QVERIFY(rows(log).contains("fm exited with code 2"));
        // One command at a time; stop ends it.
#ifdef Q_OS_WIN
        log.setCliProgram("ping"); log.run("fm -n 30 127.0.0.1");
#else
        log.setCliProgram("sleep"); log.run("fm 30");
#endif
        QVERIFY(log.cliRunning());
        log.run("fm --help");
        QVERIFY(rows(log).contains("fm is still running"));
        log.run("stop");
        QTRY_VERIFY_WITH_TIMEOUT(!log.cliRunning(), 5000);
        QVERIFY(rows(log).contains("fm stopped"));
        log.run("stop");
        QVERIFY(rows(log).contains("no fm command is running"));
        log.setCliProgram(QDir::temp().filePath("fm-missing-binary"));
        log.run("fm --help");
        QTRY_VERIFY(rows(log).contains("fm could not start"));
        QVERIFY(!log.cliRunning());
#endif
    }
    void keyboardAndProductionPanel() {
        DebugConsole log;
        log.record("WARN", "sync", "account-1 mail = retrying; google_rate_limited", {{"retry_after_secs", 23}});
        log.record("info", "IPC", "messages.get completed", {{"method", "google_sent"}, {"json", QVariantMap{{"id", "18f00"}, {"labels", QVariantList{"INBOX", "UNREAD"}}, {"size", 4000}, {"ok", true}, {"none", QVariant()}}}});
        settle();
        QTemporaryDir dir; QVERIFY(dir.isValid());
        const auto modulePath = dir.filePath("ConsoleTest"); QVERIFY(QDir().mkpath(modulePath));
        for (const auto name : {"Theme.qml", "DebugConsole.qml", "FmScrollBar.qml", "FmIcon.qml", "FmToolTip.qml"})
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
    DebugConsole { hostWindow: window; diagnostics: testDiagnostics; searchActive: window.searching; uiState: ({view: "mail", theme: window.testTheme}) }
})QML", QUrl::fromLocalFile(dir.filePath("Harness.qml")));
        QVERIFY2(!qml.rootObjects().isEmpty(), qPrintable(warnings.join('\n')));
        auto window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); QVERIFY(window);
        auto console = window->findChild<QObject *>("debugConsole"); QVERIFY(console);
        auto editor = window->findChild<QQuickItem *>("editor"); QVERIFY(editor);
        auto idle = window->findChild<QQuickItem *>("idle"); QVERIFY(idle);
        const auto center = [](QQuickItem *item) { return item->mapToScene(QPointF(item->width() / 2, item->height() / 2)).toPoint(); };
        window->requestActivate(); QTest::qWait(100);
        // ~ and ` are ignored in normal text fields and while search is open.
        editor->forceActiveFocus(); QTest::keyClick(window, Qt::Key_QuoteLeft);
        QVERIFY(!console->property("opened").toBool()); QVERIFY(editor->property("text").toString().contains('`'));
        QTest::keyClick(window, Qt::Key_AsciiTilde, Qt::ShiftModifier);
        QVERIFY(!console->property("opened").toBool()); QVERIFY(editor->property("text").toString().contains('~'));
        idle->forceActiveFocus(); window->setProperty("searching", true);
        QTest::keyClick(window, Qt::Key_QuoteLeft); QVERIFY(!console->property("opened").toBool());
        window->setProperty("searching", false);
        const auto popupState = [&] {
            auto focus = window->activeFocusItem();
            return QString("active=%8 visible=%1 opened=%2 shown=%3 reopening=%4 y=%5 focus=%6/%7").arg(console->property("visible").toBool()).arg(console->property("opened").toBool())
                .arg(console->property("shown").toBool()).arg(console->property("reopening").toBool()).arg(console->property("y").toDouble())
                .arg(focus ? focus->metaObject()->className() : "none").arg(focus ? focus->objectName() : QString()).arg(window->isActive());
        };
        QElapsedTimer slide;
        // Slides down from above the window and fills it. The first open also builds
        // the panel, so the timing is checked on a warm open.
        QTest::keyClick(window, Qt::Key_QuoteLeft);
        QTRY_VERIFY_WITH_TIMEOUT(console->property("opened").toBool() && console->property("y").toDouble() == 0.0, 2000);
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTRY_VERIFY_WITH_TIMEOUT(!console->property("visible").toBool(), 1000);
        slide.start();
        QTest::keyClick(window, Qt::Key_QuoteLeft);
        QVERIFY(console->property("y").toDouble() < 0);
        QTRY_VERIFY_WITH_TIMEOUT(console->property("opened").toBool() && console->property("y").toDouble() == 0.0, 1000);
        QVERIFY2(slide.elapsed() < 300, qPrintable(QString::number(slide.elapsed())));
        QCOMPARE(console->property("x").toDouble(), 0.0);
        QCOMPARE(console->property("width").toDouble(), 1280.0);
        QCOMPARE(console->property("height").toDouble(), 820.0);
        auto command = window->findChild<QQuickItem *>("debugCommand"); QVERIFY(command);
        QTRY_VERIFY(command->hasActiveFocus());
        auto events = window->findChild<QQuickItem *>("debugEvents"); QVERIFY(events);
        QTRY_VERIFY(events->property("count").toInt() >= 2);
        // Commands with history.
        typeText(window, "level err"); QTest::keyClick(window, Qt::Key_Return);
        QCOMPARE(log.level(), 3); QVERIFY(command->property("text").toString().isEmpty());
        typeText(window, "nonsense"); QTest::keyClick(window, Qt::Key_Return);
        QTRY_VERIFY(rows(log).contains("nonsense: command not found · type help"));
        QTest::keyClick(window, Qt::Key_Up); QCOMPARE(command->property("text").toString(), QString("nonsense"));
        QTest::keyClick(window, Qt::Key_Up); QCOMPARE(command->property("text").toString(), QString("level err"));
        QTest::keyClick(window, Qt::Key_Down); QTest::keyClick(window, Qt::Key_Down); QCOMPARE(command->property("text").toString(), QString());
        auto top = command; while (top->parentItem()) top = top->parentItem();
        auto info = findItem(top, "debugLevel_info"); QVERIFY(info);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(info));
        QCOMPARE(log.level(), 6);
        // The grep field follows `grep` typed as a command.
        command->forceActiveFocus();
        typeText(window, "grep retrying"); QTest::keyClick(window, Qt::Key_Return);
        auto grep = window->findChild<QQuickItem *>("debugGrep"); QVERIFY(grep);
        QCOMPARE(grep->property("text").toString(), QString("retrying"));
        QTRY_VERIFY(!rows(log).contains("messages.get"));
        grep->forceActiveFocus(); grep->setProperty("text", ""); QMetaObject::invokeMethod(grep, "textEdited");
        QCOMPARE(log.grep(), QString());
        // ` in the console's own fields closes it; reopen.
        slide.restart();
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTRY_VERIFY(!console->property("opened").toBool());
        // Slides back up out of view, also under 300 ms.
        QTRY_VERIFY_WITH_TIMEOUT(!console->property("visible").toBool(), 1000);
        QVERIFY2(slide.elapsed() < 300, qPrintable(QString::number(slide.elapsed())));
        QVERIFY(grep->property("text").toString().isEmpty());
        // ~ mid-slide follows what was asked for: open then close at once ends closed,
        // and ~ while it slides up brings it straight back down.
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTest::qWait(40); QTest::keyClick(window, Qt::Key_QuoteLeft);
        QTRY_VERIFY_WITH_TIMEOUT(!console->property("visible").toBool(), 1000);
        QTest::qWait(250); QVERIFY(!console->property("visible").toBool());
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTRY_VERIFY2(console->property("opened").toBool(), qPrintable(popupState()));
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTest::qWait(40); QTest::keyClick(window, Qt::Key_QuoteLeft);
        QTRY_VERIFY2_WITH_TIMEOUT(console->property("opened").toBool() && console->property("y").toDouble() == 0.0, qPrintable(popupState()), 1000);
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTRY_VERIFY_WITH_TIMEOUT(!console->property("visible").toBool(), 1000);
        QTest::keyClick(window, Qt::Key_QuoteLeft); QTRY_VERIFY(console->property("opened").toBool());
        // A JSON line: collapsed hint, expanded details and block, copy.
        QQuickItem *jsonRow = nullptr;
        QTRY_VERIFY([&] {
            auto content = events->property("contentItem").value<QQuickItem *>();
            for (auto child : content->childItems()) if (child->property("json").toString().contains("18f00")) jsonRow = child;
            return jsonRow != nullptr;
        }());
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, jsonRow->mapToScene(QPointF(400, 8)).toPoint());
        QCOMPARE(console->property("selectedSequence").toString(), jsonRow->property("sequence").toString());
        const auto sequence = jsonRow->property("sequence").toString();
        auto jsonText = findItem(top, "debugJson_" + sequence); QVERIFY(jsonText);
        QTRY_VERIFY(jsonText->isVisible());
        QVERIFY(jsonText->property("text").toString().contains("INBOX"));
        // Selecting inside the block leaves the line open. Expanding re-pins the tail, so let it settle.
        QTest::qWait(100);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(jsonText));
        QCOMPARE(console->property("selectedSequence").toString(), sequence);
        auto copy = findItem(top, "debugCopyJson_" + sequence); QVERIFY(copy);
        QTest::qWait(QGuiApplication::styleHints()->mouseDoubleClickInterval() + 50);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(copy));
        QCOMPARE(console->property("copiedSequence").toString(), sequence);
        // The system clipboard is shared with the desktop; allow it a moment.
        QTRY_VERIFY2(QGuiApplication::clipboard()->text().contains("18f00"), qPrintable(QGuiApplication::clipboard()->text().left(200)));
        const auto copied = QJsonDocument::fromJson(QGuiApplication::clipboard()->text().toUtf8()).object();
        QCOMPARE(copied.value("id").toString(), QString("18f00"));
        QVERIFY(QGuiApplication::clipboard()->text().contains("\n  \"labels\""));
        QCOMPARE(console->property("copiedSequence").toString(), sequence);
        QTRY_COMPARE_WITH_TIMEOUT(console->property("copiedSequence").toString(), QString(), 3000);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, jsonRow->mapToScene(QPointF(400, 8)).toPoint());
        QCOMPARE(console->property("selectedSequence").toString(), QString());
        // A line that fits shows its details inline and does not expand.
        QQuickItem *warnRow = nullptr;
        for (auto child : events->property("contentItem").value<QQuickItem *>()->childItems())
            if (child->property("plain").toString().contains("retry_after_secs=23")) warnRow = child;
        QVERIFY(warnRow);
        QVERIFY(warnRow->property("plain").toString().contains(" warning  sync   account-1 mail = retrying; google_rate_limited  retry_after_secs=23"));
        QVERIFY(!warnRow->property("expandable").toBool());
        QTest::qWait(QGuiApplication::styleHints()->mouseDoubleClickInterval() + 50);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, warnRow->mapToScene(QPointF(300, 8)).toPoint());
        QCOMPARE(console->property("selectedSequence").toString(), QString());
        // Pressing and dragging selects text across lines; Ctrl+C copies it.
        QTest::qWait(QGuiApplication::styleHints()->mouseDoubleClickInterval() + 50);
        const auto from = warnRow->mapToScene(QPointF(18, 8)).toPoint(), to = jsonRow->mapToScene(QPointF(jsonRow->width() - 40, 8)).toPoint();
        QGuiApplication::clipboard()->clear();
        QTest::mousePress(window, Qt::LeftButton, Qt::NoModifier, from);
        QTest::mouseMove(window, (from + to) / 2); QTest::mouseMove(window, to);
        QTest::mouseRelease(window, Qt::LeftButton, Qt::NoModifier, to);
        QCOMPARE(console->property("selectedSequence").toString(), QString());
        QTest::keySequence(window, QKeySequence::Copy);
        QTRY_VERIFY2(QGuiApplication::clipboard()->text().contains("google_rate_limited") && QGuiApplication::clipboard()->text().contains("messages.get completed"),
            qPrintable(QGuiApplication::clipboard()->text()));
        QVERIFY(QGuiApplication::clipboard()->text().contains('\n'));
        // Copy puts the filtered log on the clipboard.
        auto copyAll = window->findChild<QQuickItem *>("debugCopy"); QVERIFY(copyAll);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(copyAll));
        QTRY_VERIFY(QGuiApplication::clipboard()->text().contains("<4> warning sync    account-1 mail = retrying"));
        QTRY_VERIFY(rows(log).contains("to the clipboard"));
        // Tail: pinned while following; scrolling up stops it and counts new lines.
        for (int i = 0; i < 1200; ++i) log.record("info", "sync", QString("scroll event %1").arg(i));
        settle();
        const auto geometry = [&] { return QString("contentY=%1 height=%2 contentHeight=%3 originY=%4 follow=%5 count=%6").arg(events->property("contentY").toDouble())
            .arg(events->property("height").toDouble()).arg(events->property("contentHeight").toDouble()).arg(events->property("originY").toDouble())
            .arg(console->property("followTail").toBool()).arg(events->property("count").toInt()); };
        QTRY_VERIFY2(events->property("atYEnd").toBool(), qPrintable(geometry()));
        QCOMPARE(events->property("count").toInt(), DebugConsole::shown);
        const auto point = events->mapToScene(QPointF(400, 200));
        QWheelEvent wheel(point, window->mapToGlobal(point.toPoint()), {}, QPoint(0, 360), Qt::NoButton, Qt::NoModifier, Qt::NoScrollPhase, false);
        QTest::mouseMove(window, point.toPoint()); QTest::qWait(20);
        wheel.setTimestamp(ulong(QDateTime::currentMSecsSinceEpoch()));
        QCoreApplication::sendEvent(window, &wheel);
        QTRY_VERIFY2(!console->property("followTail").toBool(), qPrintable(geometry()));
        QTest::qWait(200);
        QVERIFY2(!events->property("atYEnd").toBool(), qPrintable(QString("contentY=%1 height=%2 contentHeight=%3 originY=%4 follow=%5").arg(events->property("contentY").toDouble())
            .arg(events->property("height").toDouble()).arg(events->property("contentHeight").toDouble()).arg(events->property("originY").toDouble()).arg(console->property("followTail").toBool())));
        auto tailButton = window->findChild<QQuickItem *>("debugTail"); QVERIFY(tailButton);
        QVERIFY(!tailButton->isVisible());
        for (int i = 0; i < 7; ++i) log.record("info", "sync", QString("new event %1").arg(i));
        settle();
        QTRY_VERIFY2(console->property("newCount").toInt() == 7, qPrintable(QString("follow=%1 appended=%2 mark=%3 new=%4 atEnd=%5").arg(console->property("followTail").toBool())
            .arg(log.appended()).arg(console->property("newMark").toDouble()).arg(console->property("newCount").toInt()).arg(events->property("atYEnd").toBool())));
        QVERIFY(tailButton->isVisible());
        QVERIFY(!events->property("atYEnd").toBool());
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(tailButton));
        QTRY_VERIFY(events->property("atYEnd").toBool());
        QVERIFY(console->property("followTail").toBool());
        QVERIFY(!tailButton->isVisible());
        for (int batch = 0; batch < 5; ++batch) {
            log.record("info", "local model", "Summary completed", {{"elapsed_ms", batch * 100}});
            settle(); QTest::qWait(30);
            QVERIFY(events->property("atYEnd").toBool());
        }
        window->resize(1000, 650); QTest::qWait(100);
        QCOMPARE(console->property("height").toDouble(), 650.0);
        QVERIFY(events->property("atYEnd").toBool());
        window->resize(1000, 400); QTest::qWait(50);
        QCOMPARE(console->property("height").toDouble(), 400.0);
        window->resize(1280, 820); QTest::qWait(100);
        QVERIFY(events->property("atYEnd").toBool());
        events->forceActiveFocus(); QTest::keyClick(window, Qt::Key_PageUp);
        QVERIFY(!console->property("followTail").toBool());
        QTest::keyClick(window, Qt::Key_End); QTRY_VERIFY(events->property("atYEnd").toBool());
        // Status bar and pause.
        auto mode = window->findChild<QQuickItem *>("debugMode"); QVERIFY(mode);
        QCOMPARE(mode->property("text").toString(), QString::fromUtf8("● tail"));
        auto pause = window->findChild<QQuickItem *>("debugPause"); QVERIFY(pause);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(pause));
        QVERIFY(log.paused()); QCOMPARE(mode->property("text").toString(), QString::fromUtf8("‖ paused"));
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(pause));
        QVERIFY(!log.paused());
        // Running a command from a scrolled-up view jumps to the bottom and stays
        // there as its output arrives, including a burst of lines later on.
        for (int i = 0; i < 700; ++i) { log.record("info", "sync", QString("filler %1").arg(i)); if (i % 40 == 0) settle(); }
        settle(); QTest::qWait(50);
        events->forceActiveFocus(); QTest::keyClick(window, Qt::Key_Home); QTest::qWait(30);
        QVERIFY(!events->property("atYEnd").toBool()); QVERIFY(!console->property("followTail").toBool());
        command->forceActiveFocus(); typeText(window, "help"); QTest::keyClick(window, Qt::Key_Return);
        QTRY_VERIFY2(events->property("atYEnd").toBool(), qPrintable(QString::number(events->property("contentY").toDouble())));
        QVERIFY(console->property("followTail").toBool());
        // Output that arrives later (as fm's does) also brings the view to the bottom.
        events->forceActiveFocus(); QTest::keyClick(window, Qt::Key_Home); QTest::qWait(30);
        QVERIFY(!events->property("atYEnd").toBool());
        log.run("level bogus");
        QTRY_VERIFY(events->property("atYEnd").toBool()); QVERIFY(console->property("followTail").toBool());
        // A burst of more than 50 lines at once resets the view's model; it stays at the end.
        QTest::qWait(100); QVERIFY(events->property("atYEnd").toBool());
        for (int i = 0; i < 80; ++i) log.record("info", "sync", QString("burst %1").arg(i));
        settle(); QTest::qWait(100);
        QVERIFY2(events->property("atYEnd").toBool(), qPrintable(QString("burst: contentY=%1 contentHeight=%2 originY=%3").arg(events->property("contentY").toDouble())
            .arg(events->property("contentHeight").toDouble()).arg(events->property("originY").toDouble())));
#ifdef FM_CLI_PATH
        events->forceActiveFocus(); QTest::keyClick(window, Qt::Key_Home); QTest::qWait(30);
        QVERIFY(!events->property("atYEnd").toBool());
        log.setCliProgram(FM_CLI_PATH);
        command->forceActiveFocus(); typeText(window, "fm mail --help"); QTest::keyClick(window, Qt::Key_Return);
        QTRY_VERIFY_WITH_TIMEOUT(!log.cliRunning(), 15000);
        QTRY_VERIFY(rows(log).contains("Usage: fm"));
        QTest::qWait(100);
        QVERIFY2(events->property("atYEnd").toBool(), qPrintable(QString("contentY=%1 contentHeight=%2 height=%3").arg(events->property("contentY").toDouble())
            .arg(events->property("contentHeight").toDouble()).arg(events->property("height").toDouble())));
        log.setPaused(false);
#endif
        // Ctrl+C in the command line ends a running fm command.
#ifdef Q_OS_WIN
        log.setCliProgram("ping"); command->forceActiveFocus(); typeText(window, "fm -n 30 127.0.0.1");
#else
        log.setCliProgram("sleep"); command->forceActiveFocus(); typeText(window, "fm 30");
#endif
        QTest::keyClick(window, Qt::Key_Return);
        QVERIFY(log.cliRunning());
        auto cliRunning = window->findChild<QQuickItem *>("debugCliRunning"); QVERIFY(cliRunning);
        QTRY_VERIFY(cliRunning->isVisible());
#ifdef Q_OS_MACOS
        QTest::keyClick(window, Qt::Key_C, Qt::MetaModifier);
#else
        QTest::keyClick(window, Qt::Key_C, Qt::ControlModifier);
#endif
        QTRY_VERIFY_WITH_TIMEOUT(!log.cliRunning(), 5000);
        QTRY_VERIFY(!cliRunning->isVisible());
        QVERIFY(rows(log).contains("fm stopped"));
        // It paused the live log; resume brings it back.
        QVERIFY(log.paused()); QCOMPARE(mode->property("text").toString(), QString::fromUtf8("‖ paused"));
        command->forceActiveFocus(); typeText(window, "resume"); QTest::keyClick(window, Qt::Key_Return);
        QVERIFY(!log.paused());
        // The state tab.
        command->forceActiveFocus(); typeText(window, "state"); QTest::keyClick(window, Qt::Key_Return);
        auto state = window->findChild<QQuickItem *>("debugState"); QVERIFY(state);
        QTRY_VERIFY(state->isVisible()); QVERIFY(!events->isVisible());
        QVERIFY(state->property("count").toInt() > 10);
        auto logsTab = findItem(top, "debugTab_logs"); QVERIFY(logsTab);
        QTest::mouseClick(window, Qt::LeftButton, Qt::NoModifier, center(logsTab));
        QTRY_VERIFY(events->isVisible());
        // Esc closes and restores focus; `exit` closes too.
        QTest::keyClick(window, Qt::Key_Escape); QTRY_VERIFY(!console->property("opened").toBool());
        QTRY_VERIFY(idle->hasActiveFocus());
        QTest::keyClick(window, Qt::Key_AsciiTilde, Qt::ShiftModifier); QTRY_VERIFY2(console->property("opened").toBool(), qPrintable(popupState()));
        QTRY_VERIFY(events->property("atYEnd").toBool());
        for (const auto theme : {"graphite", "paper", "harbour", "terminal"}) {
            window->setProperty("testTheme", theme); QTest::qWait(30);
            if (qEnvironmentVariableIsSet("FM_DEBUG_CAPTURE_DIR")) {
                const auto file = QDir(qEnvironmentVariable("FM_DEBUG_CAPTURE_DIR")).filePath(QString(theme) + ".png");
                QVERIFY(window->grabWindow().save(file));
            }
        }
        QTRY_VERIFY(command->hasActiveFocus());
        typeText(window, "exit"); QTest::keyClick(window, Qt::Key_Return);
        QTRY_VERIFY(!console->property("opened").toBool());
        QVERIFY2(warnings.isEmpty(), qPrintable(warnings.join('\n')));
    }
};
int main(int argc, char **argv) {
    QGuiApplication app(argc, argv); QQuickStyle::setStyle("Basic");
    for (const auto weight : {"Regular", "Medium", "SemiBold"}) QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf");
    DebugConsoleTest test; return QTest::qExec(&test, argc, argv);
}
#include "DebugConsoleTest.moc"
