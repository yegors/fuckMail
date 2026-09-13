#pragma once
#include <QObject>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QTimer>
class MailBackend;
class QQuickWindow;
class GuiGoogleCalendarSelfTest final : public QObject {
public:
    GuiGoogleCalendarSelfTest(MailBackend &, QQuickWindow *, const QString &expectation,
                              const QString &report, QObject *parent = nullptr);
    bool passed() const { return m_passed; }
private:
    void tick();
    void finish(const QString &error = {});
    bool click(const QString &name);
    bool openEvent(const QString &account, const QString &eventId);
    bool submit(const QString &action, int nextStage);
    MailBackend &m_backend;
    QPointer<QQuickWindow> m_window;
    QTimer m_timer, m_watchdog;
    QString m_report, m_date, m_action, m_createdId, m_responseError;
    QJsonArray m_expected, m_observed;
    QJsonObject m_limits, m_accountResult, m_initial, m_request, m_response;
    int m_stage = 0, m_account = 0, m_nextStage = 0, m_loggedStage = -1;
    bool m_quotaFilled = false, m_quotaSubmitted = false, m_responseOk = false;
    bool m_finished = false, m_passed = false;
};
