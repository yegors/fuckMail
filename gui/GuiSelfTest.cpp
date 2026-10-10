// Developer harness behind `fuckmail-gui --self-test` (CTest gui-integration): drives
// the real app on a disposable demo profile, runs the Settings, search, composition
// and calendar stages, and checks results with the `fm` CLI and fresh GUI processes.
#include "GuiSelfTest.h"
#include "GuiCalendarSelfTest.h"
#include "GuiCompositionSelfTest.h"
#include "GuiSettingsSelfTest.h"
#include "GuiMailNavigationSelfTest.h"
#include "ReplyDocument.h"
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QFile>
#include <QJsonDocument>
#include <QInputMethodEvent>
#include <QJsonArray>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QCryptographicHash>
#include <QJSValue>
#include <QQuickItem>
#include <QQuickWindow>
#include <QDebug>
#include <QDateTime>
#include <QUuid>

namespace {
QQuickItem *visualChild(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name) return parent;
    for (auto child : parent->childItems()) if (auto found = visualChild(child, name)) return found;
    return nullptr;
}
void key(QObject *target, int value) {
    QKeyEvent press(QEvent::KeyPress, value, Qt::NoModifier);
    QKeyEvent release(QEvent::KeyRelease, value, Qt::NoModifier);
    QCoreApplication::sendEvent(target, &press); QCoreApplication::sendEvent(target, &release);
}
QQuickItem *buttonWithText(QQuickItem *parent, const QString &text) {
    if (!parent || !parent->isVisible()) return nullptr;
    if (parent->metaObject()->indexOfSignal("clicked()") >= 0
        && parent->property("text").toString().remove('&') == text) return parent;
    for (auto child : parent->childItems()) if (auto found = buttonWithText(child, text)) return found;
    return nullptr;
}
QQuickItem *captureItem(QQuickItem *parent, const QString &operationId) {
    if (!parent || !parent->isVisible()) return nullptr;
    auto value = parent->property("modelData");
    if (value.metaType() == QMetaType::fromType<QJSValue>()) value = value.value<QJSValue>().toVariant();
    if (value.toMap()["operation_id"].toString() == operationId) return parent;
    for (auto child : parent->childItems()) if (auto found = captureItem(child, operationId)) return found;
    return nullptr;
}
// On screen counts: the popup reports `opened` only once it has eased in.
bool capturePopupOpen(QObject *window) {
    for (auto popup : window->findChildren<QObject *>("captureHistoryPopup"))
        if (popup->property("visible").toBool()) return true;
    return false;
}
QObject *openedDiscardDialog(QObject *window) {
    for (auto object : window->findChildren<QObject *>())
        if (object->property("title").toString() == "Discard this draft?"
            && object->property("modal").toBool() && object->property("opened").toBool()) return object;
    return nullptr;
}
bool clickVisible(QQuickWindow *window, QQuickItem *item, qreal horizontalFraction = .5) {
    if (!window || !item || !item->isVisible() || !item->isEnabled() || item->width() <= 0 || item->height() <= 0) return false;
    const auto position = item->mapToScene({item->width() * horizontalFraction, item->height() / 2});
    if (!QRectF(0, 0, window->width(), window->height()).contains(position)) return false;
    for (auto parent = item->parentItem(); parent; parent = parent->parentItem())
        if (parent->clip() && !parent->mapRectToScene(parent->boundingRect()).contains(position)) return false;
    const QPointF global = window->mapToGlobal(position.toPoint());
    QMouseEvent press(QEvent::MouseButtonPress, position, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(window, &press); QCoreApplication::sendEvent(window, &release);
    return true;
}
}

GuiSelfTest::GuiSelfTest(MailBackend &backend, QObject *window, QString profile, QString engine, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_profile(std::move(profile)), m_engine(std::move(engine)) {
    m_timer.setInterval(80);
    connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    connect(&m_backend, &MailBackend::shutdownReady, this, [this] {
        if (m_stage >= 60 && m_stage != 66) { failed("The GUI closed before the conflicting view save was merged."); return; }
        if (m_stage != 66) return;
        QProcess oracle;
        oracle.start(QDir(QFileInfo(m_engine).absolutePath()).filePath("fm"
#ifdef Q_OS_WIN
            ".exe"
#endif
        ), {"--profile", m_profile, "--engine", m_engine, "--account", "windscribe", "--json", "call", "view_get"});
        if (!oracle.waitForFinished(10000)) { failed("Independent CLI could not inspect the view after the successful close retry."); return; }
        const auto reply = QJsonDocument::fromJson(oracle.readAllStandardOutput()).object();
        const auto view = reply["result"].toObject()["view"].toObject();
        if (oracle.exitCode() != 0 || !reply["ok"].toBool() || view["listY"].toDouble() != 431 || view["calendarY"].toDouble() != 37) {
            failed(QString("Close did not merge the user's pending position with the CLI-only view field (listY %1, calendarY %2).").arg(view["listY"].toDouble()).arg(view["calendarY"].toDouble())); return;
        }
        m_backend.setProperty("selfTestCompleted", true);
        qInfo().noquote() << "GUI self-test passed: native mail/reply/calendar controls, independent CLI readback, retained validation errors, continued polling, and a quiet stale-close merge of both view fields.";
    });
    m_timer.start();
}
bool GuiSelfTest::verifyRestart() {
    // Called after the primary GUI event loop exits, while its disposable profile
    // still exists. The child must hydrate the real QML document and close cleanly.
    if (m_calendarExpected.isEmpty()) { qCritical() << "No validated edited calendar event was retained for restart verification."; return false; }
    const auto expectedPath = QDir(m_profile).filePath("calendar-restart-"+QUuid::createUuid().toString(QUuid::Id128)+".json");
    QFile expectedFile(expectedPath);
    const auto expectedBytes = QJsonDocument(QJsonObject{{"event",m_calendarExpected}}).toJson();
    if (!expectedFile.open(QIODevice::WriteOnly) || expectedFile.write(expectedBytes)!=expectedBytes.size()) {
        qCritical() << "Cannot prepare the calendar restart expectation."; return false;
    }
    expectedFile.close();
    QProcess gui;
    gui.start(QCoreApplication::applicationFilePath(), {"--smoke", "--fixture-clock", "--profile", m_profile, "--engine", m_engine,
        "--self-test-calendar-restart",expectedPath});
    if (!gui.waitForStarted(5000)) { qCritical() << "GUI restart verification could not start."; return false; }
    const auto restartedPid = QString::number(gui.processId());
    if (!gui.waitForFinished(45000)) {
        gui.kill(); gui.waitForFinished(1000); qCritical() << "GUI restart verification timed out."; return false;
    }
    if (gui.exitStatus() != QProcess::NormalExit || gui.exitCode() != 0) {
        qCritical().noquote() << "GUI restart failed:" << gui.readAllStandardError(); return false;
    }
    QFile observedFile(expectedPath+".observed.json");
    if (!observedFile.open(QIODevice::ReadOnly) || observedFile.size()>65536) {
        qCritical() << "Restarted GUI did not return a bounded calendar observation."; return false;
    }
    QJsonParseError parse;
    const auto observed = QJsonDocument::fromJson(observedFile.readAll(),&parse).object();
    if (parse.error != QJsonParseError::NoError || observed["gui_process_id"].toString()!=restartedPid
        || !observed["native_delegate_verified"].toBool() || observed["event"].toObject()!=m_calendarExpected) {
        qCritical() << "The actual restarted GUI did not preserve the exact edited calendar projection and RSVP."; return false;
    }
    const auto read = [this](const QStringList &arguments) {
        QProcess oracle;
        oracle.start(m_cli.program(), QStringList{"--profile", m_profile, "--engine", m_engine, "--account", "windscribe", "--json"} + arguments);
        if (!oracle.waitForStarted(5000) || !oracle.waitForFinished(10000)) {
            oracle.kill(); oracle.waitForFinished(1000); qCritical() << "Restart CLI readback timed out."; return QJsonObject{};
        }
        const auto reply = QJsonDocument::fromJson(oracle.readAllStandardOutput()).object();
        if (oracle.exitStatus() != QProcess::NormalExit || oracle.exitCode() != 0 || !reply["ok"].toBool()) {
            qCritical().noquote() << "Restart CLI readback failed:" << oracle.readAllStandardError(); return QJsonObject{};
        }
        return reply["result"].toObject();
    };
    const auto calendar = read({"calendar","--combined","--from","2026-09-11","--to","2026-09-12"});
    int calendarMatches = 0;
    for (const auto &value : calendar["events"].toArray()) {
        const auto event = value.toObject();
        if (event["event_id"]!=m_calendarExpected["event_id"]) continue;
        ++calendarMatches;
        if (event!=m_calendarExpected || event!=observed["event"].toObject()) {
            qCritical() << "Post-restart CLI calendar fields/version/RSVP differ from the independent expectation or restarted GUI."; return false;
        }
    }
    if (calendarMatches!=1) { qCritical() << "Post-restart CLI did not find exactly one created calendar source."; return false; }
    qInfo() << "GUI process restart preserved exact edited calendar identity, ETag, all authored fields and accepted self RSVP; visible GUI projection and independent CLI agree without duplicates.";
    const auto draft = read({"draft", "show", m_id});
    if (draft["body"].toString() != m_recoveredBody || draft["body_html"].toString() != m_recoveredHtml
        || !draft["signature_text"].toString().isEmpty()
        || draft["attachment_ids"].toArray() != QJsonArray{m_attachmentId}) {
        qCritical() << "GUI process restart changed the authored text, HTML, signature or exact attachment selection."; return false;
    }
    const auto history = read({"draft", "history", "--conversation-id", m_id})["captures"].toArray();
    int unknown = 0, accepted = 0;
    for (const auto &value : history) {
        const auto row = value.toObject();
        if (row["operation_id"].toString() == m_unknownCaptureId && row["state"].toString() == "acceptance_unknown" && row["attempts_started"].toInt() == 1) ++unknown;
        if (row["operation_id"].toString() == m_captureId && row["state"].toString() == "accepted") ++accepted;
    }
    if (history.size() != 2 || unknown != 1 || accepted != 1) { qCritical() << "GUI process restart changed capture identities or retried transport."; return false; }
    if (!m_settingsExpected.isEmpty()) {
        const auto preferences = read({"preferences", "show"})["preferences"].toObject();
        const auto expected = QJsonObject::fromVariantMap(m_settingsExpected["preferences"].toMap());
        for (auto it = expected.begin(); it != expected.end(); ++it) if (preferences[it.key()] != it.value()) {
            qCritical() << "GUI restart changed the Settings preference:" << it.key(); return false;
        }
        bool found = false;
        for (const auto &value : read({"accounts"})["accounts"].toArray()) {
            const auto account = value.toObject();
            if (account["id"].toString() != m_settingsExpected["account"].toString()) continue;
            found = account["name"].toString() == m_settingsExpected["name"].toString()
                && account["signature_text"].toString() == m_settingsExpected["signature_text"].toString();
        }
        if (!found) { qCritical() << "GUI restart did not preserve the Settings account metadata."; return false; }
        const auto snapshot = QJsonObject::fromVariantMap(m_settingsExpected["draft_snapshot"].toMap());
        for (auto it = snapshot.begin(); it != snapshot.end(); ++it) if (draft[it.key()] != it.value()) {
            qCritical() << "Account settings changed the authored snapshot after GUI restart:" << it.key(); return false;
        }
        qInfo() << "GUI process restart preserved native Settings preferences and account metadata without changing the saved draft snapshot.";
    }
    QString compositionError;
    if (!GuiCompositionSelfTest::verifyRetained(m_profile,m_engine,m_compositionExpected,compositionError)) {
        qCritical().noquote() << "Composition restart verification failed:" << compositionError; return false;
    }
    const auto originalPreferences = read({"preferences", "show"})["preferences"].toObject();
    const auto savePreferences = [&](const QJsonObject &patch) {
        const auto current = read({"preferences", "show"});
        if (!current.contains("revision")) return false;
        const auto operation = "startup-" + QUuid::createUuid().toString(QUuid::Id128);
        const auto path = QDir(m_profile).filePath(operation + ".json");
        QFile file(path);
        const auto bytes = QJsonDocument(patch).toJson();
        if (!file.open(QIODevice::WriteOnly | QIODevice::NewOnly) || file.write(bytes) != bytes.size()) return false;
        file.close();
        const auto saved = read({"preferences", "save", "--preferences-file", path,
            "--expected-revision", QString::number(current["revision"].toInteger()), "--request-id", operation});
        return saved.contains("revision");
    };
    for (const auto &scenario : {QStringList{"mail", "calendar", "mail"}, QStringList{"calendar", "mail", "calendar"},
                                QStringList{"last", "calendar", "calendar"}, QStringList{"last", "mail", "mail"}}) {
        if (!savePreferences({{"startup_view", scenario[0]}, {"last_workspace", scenario[1]}})) {
            qCritical() << "Cannot prepare the independent startup preference."; return false;
        }
        QProcess startup;
        startup.start(QCoreApplication::applicationFilePath(), {"--smoke", "--fixture-clock", "--profile", m_profile,
            "--engine", m_engine, "--self-test-startup-workspace", scenario[2]});
        if (!startup.waitForStarted(5000) || !startup.waitForFinished(45000)) {
            startup.kill(); startup.waitForFinished(1000); qCritical() << "Fresh workspace preference verification timed out."; return false;
        }
        const auto output = startup.readAllStandardError();
        if (startup.exitStatus() != QProcess::NormalExit || startup.exitCode() != 0
            || !output.contains("Fresh native process verified saved initial workspace")) {
            qCritical().noquote() << "Fresh workspace preference failed:" << output; return false;
        }
        const auto preferences = read({"preferences", "show"})["preferences"].toObject();
        if (preferences["last_workspace"].toString() != scenario[2]
            || preferences["theme"] != originalPreferences["theme"] || preferences["text_size"] != originalPreferences["text_size"]) {
            qCritical() << "Restart did not remember its workspace or changed the saved appearance."; return false;
        }
    }
    if (!savePreferences(originalPreferences)) { qCritical() << "Cannot restore native walkthrough preferences."; return false; }
    qInfo() << "Four fresh GUI processes verified Mail, Calendar and both last-workspace choices; appearance persisted and shutdown saved the resulting workspace.";
    qInfo() << "GUI process restart passed: exact authored fields and capture journal identities/attempt count preserved.";
    return true;
}
bool GuiSelfTest::membersHave(const char *flag, bool value) const {
    const auto members = m_backend.conversation().value("messages").toList();
    if (members.isEmpty()) return false;
    for (const auto &member : members) if (member.toMap().value(QLatin1String(flag)).toBool() != value) return false;
    return true;
}
bool GuiSelfTest::membersCategorized(bool present) const {
    const auto members = m_backend.conversation().value("messages").toList();
    if (members.isEmpty()) return false;
    for (const auto &member : members) {
        if (member.toMap()["categories"].toStringList().contains("finance") != present) return false;
    }
    return true;
}
bool GuiSelfTest::editorStateIntact() const {
    const auto editor = m_window->findChild<QQuickItem *>("replyComposer");
    const auto document = m_window->findChild<ReplyDocument *>("replyDocument");
    const auto intact = editor && document && document->plainText() == m_authored &&
        editor->property("cursorPosition").toInt() == 10 &&
        editor->property("selectionStart").toInt() == 6 &&
        editor->property("selectionEnd").toInt() == 10;
    if (!intact && editor) qWarning() << "Editor state: authored text matches=" << (document && document->plainText() == m_authored)
        << "cursor=" << editor->property("cursorPosition") << "selection=" << editor->property("selectionStart") << editor->property("selectionEnd");
    return intact;
}
bool GuiSelfTest::openReply() {
    const auto mail = m_window->findChild<QQuickItem *>("mailWorkspace");
    return (mail && mail->property("replyOpen").toBool()) || click("replyButton");
}
bool GuiSelfTest::click(const QString &objectName) {
    auto window = qobject_cast<QQuickWindow *>(m_window);
    auto item = window ? visualChild(window->contentItem(), objectName) : nullptr;
    if (!item || !window || !item->isVisible() || !item->isEnabled()) return false;
    const QPointF position = item->mapToScene({item->width() / 2, item->height() / 2});
    const QPointF global = window->mapToGlobal(position.toPoint());
    QMouseEvent move(QEvent::MouseMove, position, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    QMouseEvent press(QEvent::MouseButtonPress, position, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(window, &move);
    QCoreApplication::sendEvent(window, &press);
    QCoreApplication::sendEvent(window, &release);
    return true;
}
void GuiSelfTest::failed(const QString &message) {
    qCritical().noquote() << "GUI self-test failed at stage" << m_stage << ":" << message;
    m_timer.stop(); QCoreApplication::exit(5);
}
void GuiSelfTest::cli(const QStringList &arguments, std::function<void(const QJsonObject &)> callback) {
    m_timer.stop(); m_cli.disconnect(this);
    connect(&m_cli, &QProcess::finished, this, [this, callback](int code, QProcess::ExitStatus status) {
        const auto reply = QJsonDocument::fromJson(m_cli.readAllStandardOutput()).object();
        if (status != QProcess::NormalExit || code != 0 || !reply["ok"].toBool()) {
            failed("Independent CLI call failed: " + QString::fromUtf8(m_cli.readAllStandardError())); return;
        }
        callback(reply["result"].toObject()); m_timer.start();
    });
    connect(&m_cli, &QProcess::errorOccurred, this, [this](auto) { failed("Cannot run the sibling fm CLI."); });
    const auto program = QDir(QFileInfo(m_engine).absolutePath()).filePath("fm"
#ifdef Q_OS_WIN
        ".exe"
#endif
    );
    m_cli.start(program, QStringList{"--profile", m_profile, "--engine", m_engine, "--account", "windscribe", "--json"} + arguments);
}
void GuiSelfTest::cliWithParams(const QString &method, const QJsonObject &params, std::function<void(const QJsonObject &)> callback) {
    const auto path = QDir(m_profile).filePath("gui-selftest-" + method + ".json");
    QFile file(path);
    const auto bytes = QJsonDocument(params).toJson();
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size()) { failed("Cannot prepare the independent CLI request."); return; }
    file.close(); cli({"call", method, "--params-file", path}, std::move(callback));
}
QJsonObject GuiSelfTest::cliWhileGuiPaused(const QString &method, const QJsonObject &params) {
    // Deterministically interleave another interface's committed draft with this
    // editor's old revision. No GUI timers run during this test-only wait.
    const auto path = QDir(m_profile).filePath("gui-selftest-paused-" + method + ".json");
    QFile file(path); const auto bytes = QJsonDocument(params).toJson();
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size()) { failed("Cannot prepare the interleaved CLI request."); return {}; }
    file.close();
    QProcess process;
    process.start(m_cli.program(), QStringList{"--profile", m_profile, "--engine", m_engine, "--account", "windscribe", "--json", "call", method, "--params-file", path});
    if (!process.waitForStarted(5000) || !process.waitForFinished(10000)) {
        process.kill(); process.waitForFinished(1000); failed("The bounded interleaved CLI call timed out."); return {};
    }
    const auto reply = QJsonDocument::fromJson(process.readAllStandardOutput()).object();
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0 || !reply["ok"].toBool()) { failed("Interleaved CLI call failed: " + QString::fromUtf8(process.readAllStandardError())); return {}; }
    return reply["result"].toObject();
}
void GuiSelfTest::step() {
    if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE") && m_lastTracedStage != m_stage) {
        qInfo() << "GUI self-test stage" << m_stage;
        m_lastTracedStage = m_stage;
    }
    if (++m_ticks > 700) { failed(QString("Timed out waiting for the real GUI/engine state (unread=%1, starred=%2, today=%3, draftBusy=%4, draftStatus=%5, discarded=%6, draftBodyEmpty=%7).")
        .arg(membersHave("unread", true)).arg(membersHave("starred", true)).arg(m_backend.todayEvents().size())
        .arg(m_backend.draftBusy()).arg(m_backend.draftStatus()).arg(m_backend.draft()["discarded"].toBool()).arg(m_backend.draft()["body"].toString().isEmpty())); return; }
    if (!m_backend.error().isEmpty() && !m_backend.property("selfTestExpectedError").toBool()) { failed(m_backend.error()); return; }
    if (m_recreatedBackend && !m_recreatedBackend->error().isEmpty()) { failed("Reconstructed adapter: " + m_recreatedBackend->error()); return; }
    switch (m_stage) {
    case 0:
        if (!m_backend.connected() || m_backend.loading() || m_backend.conversation().isEmpty() || m_backend.draft().isEmpty()) return;
        if (m_backend.accounts().size() != 3) { failed("Expected three isolated synthetic accounts."); return; }
        {
            // Older Done months start folded; open them so paging has rows to prove.
            bool opened = false;
            for (const auto &value : m_backend.querySummary()["done_months"].toList()) {
                const auto key = "done:" + value.toMap()["month"].toString();
                if (m_backend.sectionCollapsed(key) && key != u"done:week" && key != u"done:month") { m_backend.setSectionCollapsed(key, false); opened = true; }
            }
            if (opened) return;
        }
        m_id = m_backend.selectedId();
        m_sender = m_backend.conversation()["messages"].toList().first().toMap()["sender_email"].toString();
        // Automatic prefetch can finish the second page while the selected
        // reader/draft is preparing. Both paths must still prove >50 rows below.
        if (m_backend.hasMore()) m_backend.loadMore();
        m_stage = 20; break;
    case 20:
        if (m_backend.loading()) return;
        m_loadedCount = int(m_backend.conversations().size());
        if (m_loadedCount <= 50) { failed("Load more did not add another bounded page."); return; }
        m_backend.refresh(); m_stage = 21; break;
    case 21:
        if (m_backend.loading()) return;
        if (m_backend.conversations().size() != m_loadedCount) { failed("Refreshing discarded already-visible conversation pages."); return; }
        m_backend.setRead(true); m_stage = 1; break;
    case 1:
        if (!membersHave("unread", false)) return;
        m_backend.setStarred(true); ++m_stage; break;
    case 2:
        if (!membersHave("starred", true)) return;
        if (!openReply()) return;
        m_stage = 27; break;
    case 27:
        {
            auto editor = m_window->findChild<QQuickItem *>("replyComposer");
            if (!editor) { failed("The QML reply editor is missing."); return; }
            const auto reader = m_window->findChild<QQuickItem *>("reader");
            if (!reader || !reader->mapRectToScene(reader->boundingRect()).contains(editor->mapToScene({editor->width()/2, editor->height()/2}))) return;
            editor->forceActiveFocus();
            // The reply starts with the account signature as plain text; replace it all.
            QMetaObject::invokeMethod(editor, "selectAll");
            QInputMethodEvent input; input.setCommitString(m_authored);
            QCoreApplication::sendEvent(editor, &input);
            if (m_backend.draft()["body"].toString() != m_authored) { failed("Unicode text input did not reach the durable draft adapter."); return; }
            editor->setProperty("cursorPosition", 6);
            for (int index = 0; index < 4; ++index) {
                QKeyEvent select(QEvent::KeyPress, Qt::Key_Right, Qt::ShiftModifier);
                QCoreApplication::sendEvent(editor, &select);
            }
            if (!editorStateIntact()) { failed("Native keyboard selection did not retain the authored text and cursor."); return; }
            m_stage = 22;
        }
        break;
    case 22:
        // Labels come from the reader's actions menu; the subject carries none.
        if (!QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "openActions") || !click("contextLabels")) return;
        m_stage = 23; break;
    case 23:
        if (!click("applyLabel_finance")) return;
        m_stage = 24; break;
    case 24:
        if (m_backend.mutationInFlight() || m_backend.loading() || !membersCategorized(true)) return;
        if (!editorStateIntact()) { failed("Category assignment changed the editor's text, cursor or selection."); return; }
        m_stage = 25;
        cli({"mail", "read", m_id}, [this](auto result) {
            for (const auto &member : result["messages"].toArray()) {
                if (!member.toObject()["categories"].toArray().contains("finance")) { failed("CLI did not observe the category assigned through the QML action."); return; }
            }
        });
        break;
    case 25:
        m_stage = 26;
        cli({"mail", "category", m_id, "finance", "--remove"}, [](auto) {});
        break;
    case 26:
        if (!membersCategorized(false)) return;
        if (!editorStateIntact()) { failed("The CLI category refresh changed the editor's text, cursor or selection."); return; }
        {
            QKeyEvent escape(QEvent::KeyPress, Qt::Key_Escape, Qt::NoModifier);
            QCoreApplication::sendEvent(m_window, &escape);
        }
        m_backend.saveDraft(); m_stage = 3; break;
    case 3:
        if (m_backend.draftStatus() != "Saved on this machine") return;
        ++m_stage;
        cli({"draft", "show", m_id}, [this](auto result) {
            if (result["body"].toString() != m_authored) { failed("CLI could not recover the exact Unicode draft authored through the GUI adapter."); return; }
            m_backend.setQuery(m_sender);
        });
        break;
    case 4:
        if (m_backend.loading() || m_backend.query() != m_sender || m_backend.conversations().isEmpty()) return;
        m_window->setProperty("settingsOpen", true);
        if (m_backend.account() != "windscribe" || m_backend.draft()["body"].toString() != m_authored) { failed("Entering settings changed the draft or account."); return; }
        m_window->setProperty("settingsOpen", false);
        m_backend.selectAccount("personal"); ++m_stage; break;
    case 5:
        if (m_backend.account() != "personal" || m_backend.draft().isEmpty()) return;
        if (m_backend.draft()["body"].toString() == m_authored || !m_backend.query().isEmpty()) { failed("State leaked across accounts."); return; }
        m_backend.selectAccount("windscribe"); ++m_stage; break;
    case 6:
        if (m_backend.account() != "windscribe" || m_backend.draft().isEmpty()) return;
        if (m_backend.selectedId() != m_id || m_backend.query() != m_sender || m_backend.draft()["body"].toString() != m_authored) { failed("Account navigation did not restore selection, search and durable draft."); return; }
        ++m_stage;
        cli({"mail", "flag", m_id, "--read", "false"}, [](auto) {});
        break;
    case 7:
        if (!membersHave("unread", true)) return;
        if (!membersHave("starred", true)) { failed("Changing read state cleared independent stars."); return; }
        if (m_backend.todayEvents().isEmpty()) return;
        m_window->setProperty("workspace", "calendar");
        m_backend.moveWeek(1); ++m_stage; break;
    case 8:
        if (m_backend.calendarFrom() != "2026-09-14") return;
        if (m_backend.todayEvents().isEmpty()) { failed("Browsing another week cleared today's footer."); return; }
        m_window->setProperty("settingsOpen", true);
        m_window->setProperty("settingsOpen", false);
        if (m_window->property("workspace").toString() != "calendar") { failed("Settings lost calendar navigation."); return; }
        m_backend.showFixtureWeek(); m_window->setProperty("workspace", "mail"); ++m_stage; break;
    case 9:
        if (m_backend.events().isEmpty() || m_backend.loading()) return;
        if (!click("conversationRow_" + m_id)) return;
        m_stage = 30; break;
    case 30:
        if (m_backend.mutationInFlight() || !membersHave("unread", false)) return;
        if (!membersHave("starred", true)) { failed("Read-on-open cleared held action."); return; }
        // The saved editor can reopen when returning to its conversation.
        // Minimize it through the visible control before using reader actions.
        if (auto mail = m_window->findChild<QQuickItem *>("mailWorkspace"); mail && mail->property("replyOpen").toBool()) {
            QMetaObject::invokeMethod(mail, "minimizeComposer"); return;
        }
        if (!click("doneConversationButton")) return;
        m_stage = 31; break;
    case 31:
        if (m_backend.mutationInFlight() || !membersHave("unread", false) || !membersHave("starred", false)) return;
        m_stage = 32;
        cli({"mail", "read", m_id}, [this](auto result) {
            for (const auto &member : result["messages"].toArray()) if (member.toObject()["unread"].toBool() || member.toObject()["starred"].toBool()) { failed("CLI did not observe GUI Done clearing unread and held state."); return; }
        });
        break;
    case 32:
        if (!click("settingsButton")) return;
        m_stage = 33; break;
    case 33:
        if (!click("settingsPage_Mail")) return;
        m_stage = 34; break;
    case 34:
        if (!click("densityPreference")) return;
        m_stage = 35; break;
    case 35:
        key(m_window, Qt::Key_End); key(m_window, Qt::Key_Return);
        m_stage = 36; break;
    case 36:
        if (m_backend.preferences()["density"].toString() != "compact" || !m_backend.preferencesStatus().startsWith("Saved")) return;
        m_stage = 37;
        cli({"preferences", "show"}, [this](auto result) {
            if (result["preferences"].toObject()["density"].toString() != "compact") { failed("CLI did not observe the density preference changed through the settings control."); return; }
        });
        break;
    case 37:
        if (!click("settingsDoneButton")) return;
        if (!click("replyButton")) return;
        m_stage = 38; break;
    case 38: {
        auto editor = m_window->findChild<QQuickItem *>("replyComposer");
        auto document = m_window->findChild<ReplyDocument *>("replyDocument");
        if (!editor || !document || document->plainText() != m_authored) { failed("Authored reply did not survive settings and navigation."); return; }
        QMetaObject::invokeMethod(editor, "select", Q_ARG(int, 6), Q_ARG(int, 10));
        document->format("bold");
        for (const auto &message : m_backend.conversation()["messages"].toList()) {
            for (const auto &attachment : message.toMap()["attachments"].toList()) {
                const auto item = attachment.toMap();
                if (item["state"].toString() == "available") { m_attachmentId = item["id"].toString(); break; }
            }
            if (!m_attachmentId.isEmpty()) break;
        }
        if (m_attachmentId.isEmpty()) { failed("The native reply walkthrough requires a retained attachment fixture."); return; }
        m_backend.setDraftAttachment(m_attachmentId, true); m_backend.saveDraft(); m_stage = 39; break;
    }
    case 39:
        if (m_backend.draftBusy() || m_backend.draftStatus() != "Saved on this machine") return;
        m_stage = 40;
        cli({"draft", "show", m_id}, [this](auto result) {
            if (result["body"].toString() != m_authored || !result["body_html"].toString().contains("font-weight:700") || !result["attachment_ids"].toArray().contains(m_attachmentId)) { failed("CLI did not observe the exact formatted reply and attachment selection."); return; }
        });
        break;
    case 40: {
        auto reader = m_window->findChild<QQuickItem *>("reader");
        if (!reader) { failed("Reader is missing."); return; }
        reader->setProperty("contentY", qMax(0.0, reader->property("contentHeight").toDouble() - reader->height()));
        m_stage = 41; break;
    }
    case 41:
        if (!click("captureReply")) return;
        m_stage = 42; break;
    case 42:
        if (m_backend.draftBusy() || m_backend.captures().isEmpty()) return;
        m_stage = 48;
        cli({"draft", "history", "--conversation-id", m_id}, [this](auto result) {
            const auto captures = result["captures"].toArray();
            if (captures.size() != 1 || captures[0].toObject()["state"].toString() != "accepted" || !captures[0].toObject()["local_only"].toBool() || captures[0].toObject()["provider_delivered"].toBool()) { failed("Local capture history is missing or claims provider delivery."); return; }
            m_captureId = captures[0].toObject()["operation_id"].toString();
            m_captureSha = captures[0].toObject()["mime_sha256"].toString();
            m_captureRevision = captures[0].toObject()["draft_revision"].toInteger();
            m_captureSize = captures[0].toObject()["bytes"].toInteger();
            if (m_captureId.isEmpty()) failed("Local capture has no durable operation identity.");
        });
        break;
    case 48:
        if (m_captureReturnFolder.isEmpty()) {
            m_captureReturnQuery = m_backend.query(); m_captureReturnFolder = m_backend.folder();
            m_captureListY = m_backend.presentation()["listY"].toDouble();
            m_backend.setQuery({});
        }
        if (!click("sentScopeButton")) return;
        m_stage = 90; break;
    case 90: {
        if (m_backend.loading() || m_backend.folder() != "sent" || !m_backend.query().isEmpty()) return;
        QString other;
        for (const auto &value : m_backend.conversations()) {
            const auto id = value.toMap()["id"].toString();
            if (id != m_id) { other = id; break; }
        }
        if (other.isEmpty()) { failed("Sent capture routing needs a separate outgoing conversation."); return; }
        m_backend.selectConversation(other); m_stage = 91; break;
    }
    case 91: {
        if (m_backend.loading() || m_backend.draft().isEmpty() || m_backend.selectedId() == m_id) return;
        auto list = m_window->findChild<QQuickItem *>("conversationList");
        if (!list) { failed("Sent list is missing."); return; }
        // Reach the retained operation through the actual Sent footer and
        // its secondary history popup; no backend inspect call drives this route.
        if (!QMetaObject::invokeMethod(list, "positionViewAtEnd")) { failed("Sent list could not expose its local capture footer."); return; }
        m_backend.remember("listY", list->property("contentY").toDouble() - list->property("originY").toDouble()); m_stage = 92; break;
    }
    case 92: {
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!capturePopupOpen(m_window)) { click("localCapturesButton"); return; }
        auto row = window ? captureItem(window->contentItem(), m_captureId) : nullptr;
        if (!clickVisible(window, buttonWithText(row, "Open"))) return;
        m_stage = 93; break;
    }
    case 93: {
        if (m_backend.loading() || m_backend.selectedId() != m_id || m_backend.draft().isEmpty()) return;
        if (m_backend.draft()["body"].toString() != m_authored) { failed("Local capture row did not open its original authored conversation."); return; }
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!capturePopupOpen(m_window)) {
            if (auto list=m_window->findChild<QQuickItem *>("conversationList")) QMetaObject::invokeMethod(list,"positionViewAtEnd");
            click("localCapturesButton"); return;
        }
        auto row = window ? captureItem(window->contentItem(), m_captureId) : nullptr;
        if (!clickVisible(window, buttonWithText(row, "Inspect"))) return;
        m_stage = 49; break;
    }
    case 49:
        if (m_backend.draftBusy() || !m_backend.captureStatus().contains(m_captureId)) return;
        if (!m_backend.captureStatus().contains("accepted") || !m_backend.captureStatus().contains("no email was delivered")) { failed("Native Inspect omitted the original local outcome or claimed delivery."); return; }
        if (capturePopupOpen(m_window)) key(m_window, Qt::Key_Escape);
        m_stage = 94;
        cli({"draft", "history", "--conversation-id", m_id}, [this](auto result) {
            const auto captures = result["captures"].toArray();
            if (captures.size() != 1 || captures[0].toObject()["operation_id"].toString() != m_captureId || captures[0].toObject()["state"].toString() != "accepted"
                || captures[0].toObject()["mime_sha256"].toString() != m_captureSha || captures[0].toObject()["attempts_started"].toInt() != 1) { failed("Native Inspect changed capture bytes/identity or repeated transport."); return; }
        });
        break;
    case 94:
        m_stage = 95;
        cliWithParams("draft_capture", {{"conversation_id", m_id}, {"request_id", m_captureId}, {"expected_revision", m_captureRevision}, {"outcome", "accepted"}}, [this](auto result) {
            QFile mime(result["path"].toString());
            const auto relative = QDir(QFileInfo(m_profile).canonicalFilePath()).relativeFilePath(QFileInfo(mime).canonicalFilePath());
            if (!relative.startsWith("exports/") || !relative.endsWith(".eml") || !mime.open(QIODevice::ReadOnly) || mime.size() != m_captureSize) { failed("Inspect's original operation did not resolve to its retained MIME export."); return; }
            const auto bytes = mime.readAll();
            const auto plainHeader = bytes.indexOf("Content-Type: text/plain");
            const auto plainStart = plainHeader < 0 ? -1 : bytes.indexOf("\r\n\r\n", plainHeader);
            const auto plainEnd = plainStart < 0 ? -1 : bytes.indexOf("\r\n--", plainStart + 4);
            if (plainHeader < 0 || plainStart < 0 || plainEnd < 0
                || !bytes.mid(plainHeader, plainStart - plainHeader).contains("Content-Transfer-Encoding: base64")) {
                failed("Captured MIME has no bounded base64 plain-text part."); return;
            }
            auto encoded = bytes.mid(plainStart + 4, plainEnd - plainStart - 4);
            encoded.replace("\r", ""); encoded.replace("\n", "");
            const auto authoredPart = QByteArray::fromBase64(encoded, QByteArray::AbortOnBase64DecodingErrors);
            if (result["request_id"].toString() != m_captureId || result["sha256"].toString() != m_captureSha || result["attempts_started"].toInt() != 1
                || !result["draft_unchanged"].toBool() || result["network_used"].toBool()
                || QString::fromLatin1(QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex()) != m_captureSha
                || !bytes.contains("MIME-Version: 1.0") || !bytes.contains("Content-Type: multipart/mixed") || !authoredPart.startsWith(m_authored.toUtf8())) {
                failed("Native capture inspection did not preserve the exact authored MIME and one transport attempt."); return;
            }
            qInfo() << "Native Sent/local-capture/Inspect passed: original conversation, operation identity, MIME bytes and non-delivery status verified by CLI.";
            m_backend.remember("listY", m_captureListY);
            m_backend.setFolder(m_captureReturnFolder); m_backend.setQuery(m_captureReturnQuery);
        });
        break;
    case 95:
        if (m_backend.loading() || m_backend.draftBusy()) return;
        if (m_discardId == m_id) { failed("Discard must not target the recovery draft."); return; }
        m_stage = 96;
        cli({"draft", "show", m_id}, [this](auto result) { m_protectedDraft = result; });
        break;
    case 96:
        m_stage = 97;
        cli({"mail", "read", m_discardId}, [this](auto result) { m_discardReceived = result; });
        break;
    case 97:
        m_stage = 98;
        cli({"draft", "show", m_discardId}, [this](auto result) { m_discardBefore = result; });
        break;
    case 98:
        m_stage = 99;
        cliWithParams("draft_save", {{"conversation_id", m_discardId}, {"expected_revision", m_discardBefore["revision"]}, {"request_id", "native-discard-fixture"},
            {"body", "Separate draft — discard only after my confirmation."}, {"body_html", "<p>Separate draft — <b>discard only after my confirmation.</b></p>"},
            {"signature_text", "Separate human signature"}, {"attachment_ids", QJsonArray{m_attachmentId}}}, [this](auto result) {
            m_discardBefore = result; m_backend.selectConversation(m_discardId);
        });
        break;
    case 99:
        if (m_backend.loading() || m_backend.selectedId() != m_discardId || m_backend.draft().isEmpty()) return;
        if (m_backend.draft()["body"].toString() != m_discardBefore["body"].toString()) return;
        if (!openReply()) return;
        m_stage = 100; break;
    case 100: {
        auto reader = m_window->findChild<QQuickItem *>("reader");
        if (!reader) { failed("Discard test needs the visible reply reader."); return; }
        reader->setProperty("contentY", qMax(0.0, reader->property("contentHeight").toDouble() - reader->height()));
        m_stage = 101; break;
    }
    case 101: {
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!clickVisible(window, window ? visualChild(window->contentItem(), "discardReply") : nullptr)) return;
        m_stage = 102; break;
    }
    case 102:
        if (!openedDiscardDialog(m_window)) return;
        m_stage = 103;
        cli({"draft", "show", m_discardId}, [this](auto result) {
            if (result != m_discardBefore) failed("Opening Discard confirmation already changed the saved draft.");
        });
        break;
    case 103: {
        auto dialog = openedDiscardDialog(m_window);
        auto footer = dialog ? dialog->property("footer").value<QQuickItem *>() : nullptr;
        if (!clickVisible(qobject_cast<QQuickWindow *>(m_window), buttonWithText(footer, "Cancel"))) return;
        m_stage = 104; break;
    }
    case 104:
        if (openedDiscardDialog(m_window)) return;
        m_stage = 105;
        cli({"draft", "show", m_discardId}, [this](auto result) {
            if (result != m_discardBefore || m_backend.draft()["body"].toString() != m_discardBefore["body"].toString()) failed("Cancel discarded or changed the authored draft.");
        });
        break;
    case 105: {
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!clickVisible(window, window ? visualChild(window->contentItem(), "discardReply") : nullptr)) return;
        m_stage = 106; break;
    }
    case 106: {
        auto dialog = openedDiscardDialog(m_window);
        auto footer = dialog ? dialog->property("footer").value<QQuickItem *>() : nullptr;
        if (!clickVisible(qobject_cast<QQuickWindow *>(m_window), buttonWithText(footer, "Discard"))) return;
        m_stage = 107; break;
    }
    case 107:
        if (m_backend.draftBusy() || m_backend.draftStatus() != "Draft discarded") return;
        m_stage = 108;
        cli({"draft", "show", m_discardId}, [this](auto result) {
            // A discarded draft reads like a new one: at most blank lines and the signature.
            const auto body = result["body"].toString();
            if (!(body.isEmpty() || body.startsWith("\n\n")) || body.contains(m_discardBefore["body"].toString().trimmed()) || !result["body_html"].isNull() || !result["signature_text"].toString().isEmpty()
                || !result["attachment_ids"].toArray().isEmpty() || result["revision"].toInteger() != m_discardBefore["revision"].toInteger() + 1) {
                failed("Confirmed Discard did not clear only the target's saved authored fields once."); return;
            }
        });
        break;
    case 108:
        m_stage = 109;
        cli({"mail", "read", m_discardId}, [this](auto result) {
            if (result != m_discardReceived) failed("Discard changed received source messages or flags.");
        });
        break;
    case 109:
        m_stage = 110;
        cli({"draft", "show", m_id}, [this](auto result) {
            if (result != m_protectedDraft) failed("Discard changed another conversation's saved reply.");
        });
        break;
    case 110:
        m_stage = 111;
        cli({"draft", "history", "--conversation-id", m_id}, [this](auto result) {
            const auto captures = result["captures"].toArray();
            if (captures.size() != 1 || captures[0].toObject()["operation_id"].toString() != m_captureId || captures[0].toObject()["mime_sha256"].toString() != m_captureSha) {
                failed("Discard changed existing local capture history."); return;
            }
            m_backend.selectConversation(m_id);
        });
        break;
    case 111:
        if (m_backend.loading() || m_backend.selectedId() != m_id || m_backend.draft().isEmpty()) return;
        if (m_backend.draft()["body"].toString() != m_authored || !m_backend.draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("Discard verification did not restore the untouched recovery draft."); return; }
        if (!openReply()) return;
        m_stage = 112; break;
    case 112: {
        auto editor = m_window->findChild<QQuickItem *>("replyComposer");
        auto document = m_window->findChild<ReplyDocument *>("replyDocument");
        if (!editor || !editor->isVisible() || !document || document->plainText() != m_authored) return;
        qInfo() << "Native Discard confirmation passed: opening/Cancel preserved the exact draft; confirmation cleared only its authored fields, preserving source messages, another draft and capture history.";
        m_stage = 50; break;
    }
    case 50: {
        if (m_backend.draftBusy()) return;
        const auto other = cliWhileGuiPaused("draft_save", {{"conversation_id", m_id}, {"expected_revision", m_backend.draft()["revision"].toLongLong()}, {"request_id", "native-other-draft"},
            {"body", "Other interface version"}, {"body_html", QJsonValue::Null}, {"signature_text", "Other signature"}, {"attachment_ids", QJsonArray{}}});
        if (other.isEmpty()) return;
        m_backend.setProperty("selfTestExpectedError", true);
        auto editor = m_window->findChild<QQuickItem *>("replyComposer");
        auto document = m_window->findChild<ReplyDocument *>("replyDocument");
        if (!editor || !document) { failed("Conflict test needs the real reply editor."); return; }
        editor->forceActiveFocus(); QMetaObject::invokeMethod(editor, "selectAll");
        QInputMethodEvent input; input.setCommitString(m_recoveredBody); QCoreApplication::sendEvent(editor, &input);
        QMetaObject::invokeMethod(editor, "select", Q_ARG(int, 6), Q_ARG(int, 10));
        if (!document->html().contains("font-weight:700")) document->format("bold");
        m_recoveredHtml = document->html();
        m_backend.saveDraft(); m_stage = 51; break;
    }
    case 51:
        if (m_backend.draftBusy() || !m_backend.draftConflict()) return;
        if (m_backend.draft()["body"].toString() != m_recoveredBody || m_backend.draft()["body_html"].toString() != m_recoveredHtml
            || !m_backend.draft()["signature_text"].toString().isEmpty() || !m_backend.draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("A stale save discarded local authored fields."); return; }
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError", false);
        m_stage = 52;
        cli({"draft", "show", m_id}, [this](auto result) { if (result["body"].toString() != "Other interface version\n\nOther signature") failed("Conflict handling overwrote the other saved draft without confirmation."); });
        break;
    case 52: {
        auto scroll = m_window->findChild<QQuickItem *>("replyBodyScroll");
        auto reader = m_window->findChild<QQuickItem *>("reader");
        if (!scroll || !scroll->clip() || scroll->property("contentHeight").toDouble() <= scroll->height()) { failed("Long reply text needs a clipped scrolling editor."); return; }
        reader->setProperty("contentY", qMax(0.0, reader->property("contentHeight").toDouble()-reader->height()));
        m_stage = 53; break;
    }
    case 53:
        if (!click("saveMyDraftVersion")) return;
        m_stage = 54; break;
    case 54: {
        auto dialog = m_window->findChild<QObject *>("draftConflictDialog");
        if (!dialog || !dialog->property("opened").toBool()) return;
        m_stage = 55;
        cli({"draft", "show", m_id}, [this](auto result) { if (result["body"].toString() != "Other interface version\n\nOther signature") failed("Opening conflict confirmation already overwrote the saved draft."); });
        break;
    }
    case 55:
        if (!click("confirmSaveMyDraft")) return;
        m_stage = 56; break;
    case 56:
        if (m_backend.draftBusy() || m_backend.draftConflict() || m_backend.draftStatus() != "Saved on this machine") return;
        m_stage = 57;
        cli({"draft", "show", m_id}, [this](auto result) {
            if (result["body"].toString() != m_recoveredBody || result["body_html"].toString() != m_recoveredHtml || !result["signature_text"].toString().isEmpty()
                || !result["attachment_ids"].toArray().contains(m_attachmentId)) failed("Confirmed conflict recovery did not preserve exact local authored fields.");
        });
        break;
    case 57:
        m_backend.selectAccount("personal"); m_stage = 58; break;
    case 58:
        if (m_backend.account() != "personal" || m_backend.draft().isEmpty()) return;
        m_backend.selectAccount("windscribe"); m_stage = 59; break;
    case 59:
        if (m_backend.account() != "windscribe" || m_backend.draft().isEmpty()) return;
        if (m_backend.draft()["body"].toString() != m_recoveredBody || m_backend.draft()["body_html"].toString() != m_recoveredHtml || !m_backend.draft()["signature_text"].toString().isEmpty()
            || !m_backend.draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("Recovered formatting/signature/attachments did not survive account navigation."); return; }
        m_stage = 70; break;
    case 70:
        QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "openReply"); m_stage = 71; break;
    case 71: {
        const auto button = m_window->findChild<QQuickItem *>("discardReply");
        const auto reader = m_window->findChild<QQuickItem *>("reader");
        if (!button || !reader || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() < 0
            || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() > reader->height()) return;
        QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "minimizeComposer");
        m_stage = 72; break;
    }
    case 72: {
        const auto panel = m_window->findChild<QQuickItem *>("replyPanel");
        if (!panel || panel->isVisible() || m_backend.presentation()["replyOpen"].toBool()) { failed("Collapse did not hide the nonempty saved reply."); return; }
        if (m_backend.draft()["body"].toString() != m_recoveredBody || m_backend.draft()["body_html"].toString() != m_recoveredHtml
            || !m_backend.draft()["signature_text"].toString().isEmpty() || !m_backend.draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("Collapse changed authored draft fields."); return; }
        QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "openReply"); m_stage = 73; break;
    }
    case 73: {
        const auto panel = m_window->findChild<QQuickItem *>("replyPanel");
        const auto document = m_window->findChild<ReplyDocument *>("replyDocument");
        const auto button = m_window->findChild<QQuickItem *>("discardReply");
        const auto reader = m_window->findChild<QQuickItem *>("reader");
        if (!panel || !panel->isVisible() || !button || !reader || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() < 0
            || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() > reader->height()) return;
        if (!document || document->plainText() != m_recoveredBody) { failed("Explicit Reply did not restore retained text."); return; }
        QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "minimizeComposer");
        m_captureWait.start(); m_unknownCaptureId = "native-unknown-draft"; m_stage = 74;
        cliWithParams("draft_capture", {{"conversation_id", m_id}, {"expected_revision", m_backend.draft()["revision"].toLongLong()},
            {"request_id", m_unknownCaptureId}, {"outcome", "unknown"}}, [this](auto result) {
            if (result["state"].toString() != "acceptance_unknown") { failed("Independent capture did not create the requested unknown receipt."); return; }
            m_backend.setDraftSubject(m_backend.draft()["subject"].toString() + " (newer)"); m_backend.saveDraft();
        });
        break;
    }
    case 74:
        if (m_backend.draftBusy() || m_backend.draftStatus() != "Saved on this machine" || m_captureWait.elapsed() < 1000) return;
        m_stage = 75;
        cli({"call", "view_get"}, [this](auto result) {
            const auto view = result["view"].toObject();
            if (view["selected"].toString() != m_id || !view.contains("replyOpen") || view["replyOpen"].toBool()) { failed("Collapsed reply identity did not persist for restart."); return; }
            m_recreatedBackend = new MailBackend(m_profile, m_engine, this, true);
            connect(m_recreatedBackend, &MailBackend::shutdownReady, this, [this] { m_recreatedClosed = true; });
            m_recreatedBackend->start();
        });
        break;
    case 75:
        if (!m_recreatedBackend || !m_recreatedBackend->connected() || m_recreatedBackend->accounts().isEmpty()) return;
        if (m_recreatedBackend->account() != "windscribe") m_recreatedBackend->selectAccount("windscribe");
        m_stage = 76; break;
    case 76:
        if (m_recreatedBackend->loading() || m_recreatedBackend->draft().isEmpty()) return;
        if (m_recreatedBackend->selectedId() != m_id) { failed("Reconstructed adapter did not restore the collapsed draft identity."); return; }
        if (m_recreatedBackend->presentation()["replyOpen"].toBool()) { failed("Reconstructed adapter reopened the collapsed draft."); return; }
        m_recreatedBackend->captureDraft(); m_stage = 77; break;
    case 77:
        if (m_recreatedBackend->draftBusy() || !m_recreatedBackend->captureStatus().contains("acceptance_unknown")) return;
        if (!m_recreatedBackend->captureStatus().contains("earlier saved revision") || m_recreatedBackend->draft()["body"].toString() != m_recoveredBody
            || m_recreatedBackend->draft()["body_html"].toString() != m_recoveredHtml || !m_recreatedBackend->draft()["signature_text"].toString().isEmpty()
            || !m_recreatedBackend->draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("Recovery changed the newer authored draft or omitted its earlier-revision explanation."); return; }
        m_stage = 78;
        cli({"draft", "history", "--conversation-id", m_id}, [this](auto result) {
            const auto records = result["captures"].toArray(); int unknown = 0, accepted = 0;
            for (const auto &value : records) {
                const auto row = value.toObject();
                if (row["operation_id"].toString() == m_unknownCaptureId && row["state"].toString() == "acceptance_unknown" && row["attempts_started"].toInt() == 1) ++unknown;
                if (row["operation_id"].toString() == m_captureId && row["state"].toString() == "accepted") ++accepted;
            }
            if (records.size() != 2 || unknown != 1 || accepted != 1) failed("Reconstructed capture minted a duplicate identity or repeated the unknown transport attempt.");
        });
        break;
    case 78:
        m_stage = 79;
        cli({"draft", "show", m_id}, [this](auto result) {
            if (result["body"].toString() != m_recoveredBody || result["body_html"].toString() != m_recoveredHtml
                || !result["signature_text"].toString().isEmpty() || !result["attachment_ids"].toArray().contains(m_attachmentId)) { failed("Capture recovery changed persisted current authored fields."); return; }
            // Exercise teardown with a live socket; its destructor must not
            // invoke callbacks into already-destroyed request/view storage.
            delete m_recreatedBackend; m_recreatedBackend = nullptr; m_recreatedClosed = true;
        });
        break;
    case 79:
        if (!m_recreatedClosed) return;
        if (m_recreatedBackend) m_recreatedBackend->deleteLater();
        m_recreatedBackend = nullptr;
        m_stage = 80; break;
    case 80: {
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!window) { failed("Settings controls need the native window."); return; }
        m_timer.stop();
        new GuiSettingsSelfTest(m_backend, window, m_profile, m_engine, [this](const auto &result, const auto &error) {
            if (!error.isEmpty()) { failed(error); return; }
            setExpectedSettings(result); m_stage = 81; m_timer.start();
        }, this);
        break;
    }
    case 81: {
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!window) { failed("Search navigation needs the native window."); return; }
        m_timer.stop();
        new GuiMailNavigationSelfTest(m_backend, window, [this](const auto &error) {
            if (!error.isEmpty()) { failed(error); return; }
            m_stage = 82; m_timer.start();
        }, this);
        break;
    }
    case 82: {
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!window) { failed("Composition controls need the native window."); return; }
        m_timer.stop();
        new GuiCompositionSelfTest(m_backend, window, m_profile, m_engine, [this](const auto &result, const auto &error) {
            if (!error.isEmpty()) { failed(error); return; }
            m_compositionExpected = result;
            m_stage = 43; m_timer.start();
        }, this);
        break;
    }
    case 43: {
        auto window = qobject_cast<QQuickWindow *>(m_window);
        if (!window) { failed("Calendar controls need the native window."); return; }
        m_timer.stop();
        new GuiCalendarSelfTest(m_backend, window, m_profile, m_engine, [this](const auto &result, const auto &error) {
            if (!error.isEmpty()) { failed(error); return; }
            m_calendarResult = result; m_stage = 46; m_timer.start();
        }, this);
        break;
    }
    case 46:
        m_stage = 47;
        cli({"calendar", "--combined", "--from", "2026-09-11", "--to", "2026-09-12"}, [this](auto result) {
            int found = 0;
            for (const auto &event : result["events"].toArray()) if (event.toObject()["event_id"].toString() == m_calendarResult["target"].toMap()["event"].toString()) {
                ++found;
                const auto source = event.toObject();
                const auto key = source["occurrence_key"].toObject();
                const QJsonArray attendees{QJsonObject{{"email","you@windscribe.example"},{"self",true},{"responseStatus","accepted"}}};
                if (source["title"].toString() != "Edited native walkthrough event" || source["self_response"].toString() != "accepted"
                    || source["account"].toString() != "windscribe" || source["calendar_id"].toString() != "primary"
                    || source["location"].toString() != "Fixture room" || source["description"].toString() != "Human calendar edit — Montréal 日本語"
                    || source["conference_url"].toString() != "https://example.test/meeting" || source["time_zone"].toString() != "America/Toronto"
                    || QDateTime::fromString(source["start"].toString(),Qt::ISODate).toUTC()!=QDateTime::fromString("2026-09-11T19:30:00Z",Qt::ISODate)
                    || QDateTime::fromString(source["end"].toString(),Qt::ISODate).toUTC()!=QDateTime::fromString("2026-09-11T20:00:00Z",Qt::ISODate)
                    || source["all_day"]!=false || source["recurring"]!=false || source["busy_only"]!=false || source["read_only"]!=false
                    || source["can_edit"]!=true || source["can_rsvp"]!=true || source["attendees"].toArray()!=attendees
                    || source["etag"].toString().isEmpty() || source["etag"].toString()!=m_calendarResult["etag"].toString()
                    || source["id"].toString().isEmpty() || key["account"].toString()!="windscribe" || key["calendar"].toString()!="primary"
                    || key["series"]!=source["event_id"] || !key["original_start"].isNull()) {
                    failed("CLI calendar did not observe the independently expected edited source fields, exact identity/version and self RSVP."); return;
                }
                m_calendarExpected = source;
            }
            if (found != 1) { failed("Calendar create/edit/RSVP did not produce exactly one CLI occurrence."); return; }
        });
        break;
    case 47:
        if (m_backend.preferencesPending() || m_backend.draftBusy() || m_backend.loading()) return;
        m_stage = 60;
        cli({"call", "view_get"}, [this](auto result) {
            auto view = result["view"].toObject(); view["calendarY"] = 37;
            m_shutdownViewParams = {{"request_id", "native-view-conflict"}, {"expected_revision", result["revision"]}, {"view", view}};
        });
        break;
    case 60:
        m_stage = 61;
        cliWithParams("view_save", m_shutdownViewParams, [this](auto) {});
        break;
    case 61:
        // The CLI saved first: the GUI's next view save meets a stale revision.
        m_shutdownExpectedStar = !membersHave("starred", true); m_stage = 62;
        cli({"mail", m_shutdownExpectedStar ? "delay" : "undelay", m_backend.selectedId()}, [this](auto) {});
        break;
    case 62:
        if (!membersHave("starred", m_shutdownExpectedStar)) return;
        // No explicit refresh above: this observes the GUI status poll.
        m_stage = 63; break;
    case 63:
        // Close merges the pending position into the CLI's newer view.
        m_backend.remember("listY", 431);
        m_stage = 66; m_timer.stop(); m_backend.shutdown(); break;
    }
}
