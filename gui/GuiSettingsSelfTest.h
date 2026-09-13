#pragma once
#include "MailBackend.h"
#include <QQuickWindow>
#include <QProcess>
#include <QTimer>

// Native Settings controls and independent CLI readback on a disposable profile.
class GuiSettingsSelfTest final : public QObject {
public:
    GuiSettingsSelfTest(MailBackend &backend, QQuickWindow *window, QString profile, QString engine,
        std::function<void(const QVariantMap &, const QString &)> finished, QObject *parent);
private:
    void step();
    void finish(const QString &error = {});
    QQuickItem *item(const QString &name) const;
    bool click(const QString &name);
    bool type(const QString &name, const QString &text);
    bool choose(const QString &name, int index);
    bool saved() const;
    bool draftIntact() const;
    void cli(const QStringList &arguments, std::function<void(const QJsonObject &)> callback);
    MailBackend &m_backend;
    QQuickWindow *m_window;
    QString m_profile, m_engine, m_id;
    std::function<void(const QVariantMap &, const QString &)> m_finished;
    QTimer m_timer;
    QElapsedTimer m_wait;
    QProcess m_cli;
    QVariantMap m_result;
    QVariantMap m_originalPreferences;
    QJsonObject m_draftSnapshot;
    QString m_originalName, m_originalSignature;
    const QString m_name = "Windscribe — Montréal";
    const QString m_signature = "Native settings — Montréal\nA human-written default signature.";
    const QString m_oversizedSignature = QString(QChar(0x03bb)).repeated(9000);
    double m_listY = 0, m_readerY = 0;
    bool m_editorPrepared = false;
    int m_defaultFontSize = 0;
    int m_calendarSnapshotGeneration = 0;
    qint64 m_appearanceRevision = -1;
    int m_stage = 0, m_ticks = 0, m_lastStage = -1;
};
