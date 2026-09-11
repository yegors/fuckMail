#include "GuiSelfTest.h"
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
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
}

GuiSelfTest::GuiSelfTest(MailBackend &backend, QObject *window, QString profile, QString engine, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_profile(std::move(profile)), m_engine(std::move(engine)) {
    m_timer.setInterval(80);
    connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    m_timer.start();
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
    const auto intact = editor && editor->property("text").toString() == m_authored &&
        editor->property("cursorPosition").toInt() == 10 &&
        editor->property("selectionStart").toInt() == 6 &&
        editor->property("selectionEnd").toInt() == 10;
    if (!intact && editor) qWarning() << "Editor state: authored text matches=" << (editor->property("text").toString() == m_authored)
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
void GuiSelfTest::step() {
    if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE") && m_lastTracedStage != m_stage) {
        qInfo() << "GUI self-test stage" << m_stage;
        m_lastTracedStage = m_stage;
    }
    if (++m_ticks > 700) { failed(QString("Timed out waiting for the real GUI/engine state (unread=%1, starred=%2, today=%3).").arg(membersHave("unread", true)).arg(membersHave("starred", true)).arg(m_backend.todayEvents().size())); return; }
    if (!m_backend.error().isEmpty()) { failed(m_backend.error()); return; }
    switch (m_stage) {
    case 0:
        if (!m_backend.connected() || m_backend.conversation().isEmpty() || m_backend.draft().isEmpty()) return;
        if (m_backend.accounts().size() != 2) { failed("Expected two isolated synthetic accounts."); return; }
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
        {
            auto editor = m_window->findChild<QQuickItem *>("replyComposer");
            if (!editor) { failed("The QML reply editor is missing."); return; }
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
        if (m_backend.weekStart() != "2026-09-14" || !m_backend.events().isEmpty()) return;
        if (m_backend.todayEvents().isEmpty()) { failed("Browsing another week cleared today's footer."); return; }
        m_window->setProperty("settingsOpen", true);
        m_window->setProperty("settingsOpen", false);
        if (m_window->property("workspace").toString() != "calendar") { failed("Settings lost calendar navigation."); return; }
        m_backend.showFixtureWeek(); m_window->setProperty("workspace", "mail"); ++m_stage; break;
    case 9:
        if (m_backend.events().isEmpty()) return;
        qInfo().noquote() << "GUI self-test passed: real IPC, bounded paging retained on refresh, explicit flags/categories with CLI readback, mouse category action and keyboard selection preservation, CLI-observed Unicode draft, account/search/selection restoration, settings and calendar/today isolation.";
        m_timer.stop(); m_backend.shutdown(); break;
    }
}
