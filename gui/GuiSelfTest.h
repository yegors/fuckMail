#pragma once
#include <QObject>
#include <QProcess>
#include <QTimer>
#include <QElapsedTimer>
#include "MailBackend.h"

// Runs only against a newly allocated disposable profile via --self-test.
// Uses the real local engine and an independent real CLI process.
class GuiSelfTest final : public QObject {
public:
    GuiSelfTest(MailBackend &backend, QObject *window, QString profile, QString engine, QObject *parent);
    bool verifyRestart();
    void setExpectedSettings(const QVariantMap &expected) { m_settingsExpected = expected; }
private:
    void step();
    void cli(const QStringList &arguments, std::function<void(const QJsonObject &)> callback);
    void cliWithParams(const QString &method, const QJsonObject &params, std::function<void(const QJsonObject &)> callback);
    QJsonObject cliWhileGuiPaused(const QString &method, const QJsonObject &params);
    void failed(const QString &message);
    bool membersHave(const char *flag, bool value) const;
    bool membersCategorized(bool present) const;
    bool editorStateIntact() const;
    bool click(const QString &objectName);
    MailBackend &m_backend;
    QObject *m_window;
    QString m_profile, m_engine, m_id, m_sender;
    QString m_attachmentId, m_captureId;
    QVariantMap m_calendarResult;
    QVariantMap m_settingsExpected;
    QJsonObject m_shutdownViewParams;
    bool m_shutdownExpectedStar = false;
    const QString m_authored = "Hello Maya,\n\nConfirmed — Montréal at 14:00.\nI wrote this reply myself.\n";
    const QString m_recoveredBody = m_authored + QString("Retained local detail — Montréal 日本語.\n").repeated(28);
    const QString m_recoveredSignature = "Human signature — local conflict version";
    QString m_recoveredHtml;
    MailBackend *m_recreatedBackend = nullptr;
    bool m_recreatedClosed = false;
    QString m_unknownCaptureId;
    QElapsedTimer m_captureWait;
    int m_stage = 0, m_ticks = 0, m_loadedCount = 0;
    int m_lastTracedStage = -1;
    QTimer m_timer;
    QProcess m_cli;
};
