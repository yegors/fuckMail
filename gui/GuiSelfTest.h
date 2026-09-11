#pragma once
#include <QObject>
#include <QProcess>
#include <QTimer>
#include "MailBackend.h"

// Runs only against a newly allocated disposable profile via --self-test.
// Uses the real local engine and an independent real CLI process.
class GuiSelfTest final : public QObject {
public:
    GuiSelfTest(MailBackend &backend, QObject *window, QString profile, QString engine, QObject *parent);
private:
    void step();
    void cli(const QStringList &arguments, std::function<void(const QJsonObject &)> callback);
    void failed(const QString &message);
    bool membersHave(const char *flag, bool value) const;
    MailBackend &m_backend;
    QObject *m_window;
    QString m_profile, m_engine, m_id, m_sender;
    const QString m_authored = "Hello Maya,\n\nConfirmed — Montréal at 14:00.\nI wrote this reply myself.\n";
    int m_stage = 0, m_ticks = 0, m_loadedCount = 0;
    QTimer m_timer;
    QProcess m_cli;
};
