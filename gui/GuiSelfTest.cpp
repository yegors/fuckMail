#include "GuiSelfTest.h"
#include "GuiCalendarSelfTest.h"
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
#include <QQuickItem>
#include <QQuickWindow>
#include <QDebug>

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
}

GuiSelfTest::GuiSelfTest(MailBackend &backend, QObject *window, QString profile, QString engine, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_profile(std::move(profile)), m_engine(std::move(engine)) {
    m_timer.setInterval(80);
    connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    connect(&m_backend, &MailBackend::shutdownReady, this, [this] {
        if (m_stage >= 60 && m_stage != 66) { failed("Conflicting view save closed the GUI before recovery."); return; }
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
            failed("Close retry did not preserve both the user's pending position and the CLI-only view field."); return;
        }
        m_backend.setProperty("selfTestCompleted", true);
        qInfo().noquote() << "GUI self-test passed: native mail/reply/calendar controls, independent CLI readback, retained validation errors, and stale-close recovery with continued polling and merged view persistence.";
    });
    m_timer.start();
}
bool GuiSelfTest::verifyRestart() {
    // Called after the primary GUI event loop exits, while its disposable profile
    // still exists. The child must hydrate the real QML document and close cleanly.
    QProcess gui;
    gui.start(QCoreApplication::applicationFilePath(), {"--smoke", "--fixture-clock", "--profile", m_profile, "--engine", m_engine});
    if (!gui.waitForStarted(5000) || !gui.waitForFinished(45000)) {
        gui.kill(); gui.waitForFinished(1000); qCritical() << "GUI restart verification timed out."; return false;
    }
    if (gui.exitStatus() != QProcess::NormalExit || gui.exitCode() != 0) {
        qCritical().noquote() << "GUI restart failed:" << gui.readAllStandardError(); return false;
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
    const auto draft = read({"draft", "show", m_id});
    if (draft["body"].toString() != m_recoveredBody || draft["body_html"].toString() != m_recoveredHtml
        || draft["signature_text"].toString() != m_recoveredSignature + "\nNewer unsent signature."
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
bool GuiSelfTest::click(const QString &objectName) {
    auto window = qobject_cast<QQuickWindow *>(m_window);
    auto item = window ? visualChild(window->contentItem(), objectName) : nullptr;
    if (!item || !window || !item->isVisible() || !item->isEnabled()) return false;
    const QPointF position = item->mapToScene({item->width() / 2, item->height() / 2});
    const QPointF global = window->mapToGlobal(position.toPoint());
    QMouseEvent press(QEvent::MouseButtonPress, position, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
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
    if (++m_ticks > 700) { failed(QString("Timed out waiting for the real GUI/engine state (unread=%1, starred=%2, today=%3).").arg(membersHave("unread", true)).arg(membersHave("starred", true)).arg(m_backend.todayEvents().size())); return; }
    if (!m_backend.error().isEmpty() && !m_backend.property("selfTestExpectedError").toBool()) { failed(m_backend.error()); return; }
    if (m_recreatedBackend && !m_recreatedBackend->error().isEmpty()) { failed("Reconstructed adapter: " + m_recreatedBackend->error()); return; }
    switch (m_stage) {
    case 0:
        if (!m_backend.connected() || m_backend.loading() || m_backend.conversation().isEmpty() || m_backend.draft().isEmpty()) return;
        if (m_backend.accounts().size() != 3) { failed("Expected three isolated synthetic accounts."); return; }
        m_id = m_backend.selectedId();
        m_sender = m_backend.conversation()["messages"].toList().first().toMap()["sender_email"].toString();
        if (!m_backend.hasMore()) { failed("The fixture must exercise more than one result page."); return; }
        m_backend.loadMore(); m_stage = 20; break;
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
        if (!click("replyButton")) return;
        m_stage = 27; break;
    case 27:
        {
            auto editor = m_window->findChild<QQuickItem *>("replyComposer");
            if (!editor) { failed("The QML reply editor is missing."); return; }
            const auto reader = m_window->findChild<QQuickItem *>("reader");
            if (!reader || !reader->mapRectToScene(reader->boundingRect()).contains(editor->mapToScene({editor->width()/2, editor->height()/2}))) return;
            editor->forceActiveFocus();
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
        if (!click("categoryAssignmentButton")) return;
        m_stage = 23; break;
    case 23:
        if (!click("categoryAdd_finance")) return;
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
            if (m_captureId.isEmpty()) failed("Local capture has no durable operation identity.");
        });
        break;
    case 48:
        m_backend.inspectCapture(m_captureId); m_stage = 49; break;
    case 49:
        if (m_backend.draftBusy() || !m_backend.captureStatus().contains(m_captureId)) return;
        m_stage = 50;
        cli({"draft", "history", "--conversation-id", m_id}, [this](auto result) {
            const auto captures = result["captures"].toArray();
            if (captures.size() != 1 || captures[0].toObject()["operation_id"].toString() != m_captureId || captures[0].toObject()["state"].toString() != "accepted") { failed("Inspecting an accepted capture changed its identity or created a duplicate."); return; }
        });
        break;
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
        m_backend.setDraftSignature(m_recoveredSignature); m_backend.saveDraft(); m_stage = 51; break;
    }
    case 51:
        if (m_backend.draftBusy() || !m_backend.draftConflict()) return;
        if (m_backend.draft()["body"].toString() != m_recoveredBody || m_backend.draft()["body_html"].toString() != m_recoveredHtml
            || m_backend.draft()["signature_text"].toString() != m_recoveredSignature || !m_backend.draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("A stale save discarded local authored fields."); return; }
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError", false);
        m_stage = 52;
        cli({"draft", "show", m_id}, [this](auto result) { if (result["body"].toString() != "Other interface version") failed("Conflict handling overwrote the other saved draft without confirmation."); });
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
        cli({"draft", "show", m_id}, [this](auto result) { if (result["body"].toString() != "Other interface version") failed("Opening conflict confirmation already overwrote the saved draft."); });
        break;
    }
    case 55:
        if (!click("confirmSaveMyDraft")) return;
        m_stage = 56; break;
    case 56:
        if (m_backend.draftBusy() || m_backend.draftConflict() || m_backend.draftStatus() != "Saved on this machine") return;
        m_stage = 57;
        cli({"draft", "show", m_id}, [this](auto result) {
            if (result["body"].toString() != m_recoveredBody || result["body_html"].toString() != m_recoveredHtml || result["signature_text"].toString() != m_recoveredSignature
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
        if (m_backend.draft()["body"].toString() != m_recoveredBody || m_backend.draft()["body_html"].toString() != m_recoveredHtml || m_backend.draft()["signature_text"].toString() != m_recoveredSignature
            || !m_backend.draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("Recovered formatting/signature/attachments did not survive account navigation."); return; }
        m_stage = 70; break;
    case 70:
        QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "openReply"); m_stage = 71; break;
    case 71: {
        const auto button = m_window->findChild<QQuickItem *>("collapseReply");
        const auto reader = m_window->findChild<QQuickItem *>("reader");
        if (!button || !reader || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() < 0
            || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() > reader->height()) return;
        if (!click("collapseReply")) return;
        m_stage = 72; break;
    }
    case 72: {
        const auto panel = m_window->findChild<QQuickItem *>("replyPanel");
        if (!panel || panel->isVisible() || m_backend.presentation()["replyOpen"].toBool()) { failed("Collapse did not hide the nonempty saved reply."); return; }
        if (m_backend.draft()["body"].toString() != m_recoveredBody || m_backend.draft()["body_html"].toString() != m_recoveredHtml
            || m_backend.draft()["signature_text"].toString() != m_recoveredSignature || !m_backend.draft()["attachment_ids"].toList().contains(m_attachmentId)) { failed("Collapse changed authored draft fields."); return; }
        QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "openReply"); m_stage = 73; break;
    }
    case 73: {
        const auto panel = m_window->findChild<QQuickItem *>("replyPanel");
        const auto document = m_window->findChild<ReplyDocument *>("replyDocument");
        const auto button = m_window->findChild<QQuickItem *>("collapseReply");
        const auto reader = m_window->findChild<QQuickItem *>("reader");
        if (!panel || !panel->isVisible() || !button || !reader || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() < 0
            || button->mapToItem(reader, QPointF(button->width()/2,button->height()/2)).y() > reader->height()) return;
        if (!document || document->plainText() != m_recoveredBody) { failed("Explicit Reply did not restore retained text."); return; }
        if (!click("collapseReply")) return;
        m_captureWait.start(); m_unknownCaptureId = "native-unknown-draft"; m_stage = 74;
        cliWithParams("draft_capture", {{"conversation_id", m_id}, {"expected_revision", m_backend.draft()["revision"].toLongLong()},
            {"request_id", m_unknownCaptureId}, {"outcome", "unknown"}}, [this](auto result) {
            if (result["state"].toString() != "acceptance_unknown") { failed("Independent capture did not create the requested unknown receipt."); return; }
            m_backend.setDraftSignature(m_recoveredSignature + "\nNewer unsent signature."); m_backend.saveDraft();
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
            || m_recreatedBackend->draft()["body_html"].toString() != m_recoveredHtml || m_recreatedBackend->draft()["signature_text"].toString() != m_recoveredSignature + "\nNewer unsent signature."
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
                || result["signature_text"].toString() != m_recoveredSignature + "\nNewer unsent signature." || !result["attachment_ids"].toArray().contains(m_attachmentId)) { failed("Capture recovery changed persisted current authored fields."); return; }
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
                if (event.toObject()["title"].toString() != "Edited native walkthrough event" || event.toObject()["self_response"].toString() != "accepted"
                    || event.toObject()["account"].toString() != "windscribe" || event.toObject()["location"].toString() != "Fixture room"
                    || event.toObject()["description"].toString() != "Human calendar edit — Montréal 日本語") { failed("CLI calendar did not observe the native event fields and RSVP."); return; }
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
        m_backend.setProperty("selfTestExpectedError", true);
        m_backend.remember("listY", 431); m_stage = 62; m_backend.shutdown(); break;
    case 62:
        if (!m_backend.error().contains("current mail view was not saved")) return;
        if (!m_backend.connected()) { failed("A failed close lost the live GUI connection."); return; }
        if (m_backend.presentation()["calendarY"].toDouble() != 37 || m_backend.presentation()["listY"].toDouble() != 431) return;
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError", false);
        m_shutdownExpectedStar = !membersHave("starred", true); m_stage = 63;
        cli({"mail", m_shutdownExpectedStar ? "delay" : "undelay", m_backend.selectedId()}, [this](auto) {});
        break;
    case 63:
        if (!membersHave("starred", m_shutdownExpectedStar)) return;
        // No explicit refresh above: this observes the resumed GUI status poll.
        m_stage = 66; m_timer.stop(); m_backend.shutdown(); break;
    }
}
