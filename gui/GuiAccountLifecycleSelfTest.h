#pragma once
#include <QObject>
#include <QElapsedTimer>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QTimer>
#include <QUrl>
class MailBackend;
class QQuickWindow;
class GuiAccountLifecycleSelfTest final : public QObject {
    Q_OBJECT
public:
    GuiAccountLifecycleSelfTest(MailBackend &, QQuickWindow *, const QString &expectation, const QString &report, QObject *parent = nullptr);
    ~GuiAccountLifecycleSelfTest() override;
    bool passed() const { return m_passed; }
public slots:
    void acceptBrowserUrl(const QUrl &);
    void observedControlClick();
private:
    void tick();
    void finish(const QString &error = {});
    bool click(const QString &name);
    bool type(const QString &name, const QString &text, bool accept = false);
    bool step(const QString &identity, bool renamed = false);
    bool marker(const QString &name, bool present);
    bool clickDraft(const QString &id);
    QJsonObject cli(const QString &account, const QStringList &arguments);
    QJsonObject diagnosticState() const;
    bool captureProviderBeforeRestore();
    MailBackend &m_backend;
    QPointer<QQuickWindow> m_window;
    QTimer m_timer, m_watchdog;
    QElapsedTimer m_elapsed;
    QString m_report, m_cli, m_conversation, m_date;
    QJsonArray m_expected, m_results;
    QJsonArray m_stages, m_dispatches, m_clickedSignals;
    QJsonObject m_limits, m_confirmation, m_external, m_buffer, m_unreadBefore, m_disposition;
    QJsonObject m_providerBeforeRestore;
    qint64 m_beforeRestoreCutoff = 0;
    int m_stage = 0, m_loggedStage = -1, m_account = 0, m_step = 0, m_browserCount = 0;
    int m_droppedStages = 0, m_droppedDispatches = 0, m_droppedClickedSignals = 0;
    bool m_probe = false, m_finished = false, m_passed = false;
};
