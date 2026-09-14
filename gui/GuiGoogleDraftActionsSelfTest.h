#pragma once
#include <QObject>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QTimer>
class MailBackend;
class QQuickWindow;
class GuiGoogleDraftActionsSelfTest final : public QObject {
public:
    GuiGoogleDraftActionsSelfTest(MailBackend &, QQuickWindow *, const QString &expectation,
                                  const QString &report, QObject *parent = nullptr);
    bool passed() const { return m_passed; }
private:
    void tick();
    void finish(const QString &error = {});
    bool click(const QString &name);
    MailBackend &m_backend;
    QPointer<QQuickWindow> m_window;
    QTimer m_timer, m_watchdog;
    QString m_report, m_sourceId, m_requestId;
    QJsonArray m_expected, m_observed, m_exports;
    QJsonObject m_accountResult, m_limits;
    int m_stage=0, m_account=0, m_action=0, m_pages=0, m_loggedStage=-1;
    bool m_quotaFilled=false, m_quotaSubmitted=false;
    bool m_finished=false, m_passed=false;
};
