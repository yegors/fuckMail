#include "GuiSelfTest.h"
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QJsonDocument>
#include <QInputMethodEvent>
#include <QQuickItem>
#include <QDebug>

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
            m_backend.saveDraft(); ++m_stage;
        }
        break;
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
        qInfo().noquote() << "GUI self-test passed: real IPC, bounded paging retained on refresh, explicit independent flags, CLI-observed Unicode draft, account/search/selection restoration, settings and calendar/today isolation.";
        m_timer.stop(); m_backend.shutdown(); break;
    }
}
