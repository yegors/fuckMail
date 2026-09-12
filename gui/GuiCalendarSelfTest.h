#pragma once
#include "MailBackend.h"
#include <QQuickWindow>
#include <QProcess>
#include <QPointer>
#include <QTimer>

// Disposable-profile walkthrough through actual native calendar controls.
class GuiCalendarSelfTest final : public QObject {
public:
    GuiCalendarSelfTest(MailBackend &backend, QQuickWindow *window,
        QString profile, QString engine, std::function<void(const QVariantMap &, const QString &)> finished, QObject *parent);
    static bool verifyRestartProjection(MailBackend &backend, QQuickWindow *window,
        const QString &expectedPath, QString &error);
private:
    void step();
    void finish(const QString &error = {});
    QQuickItem *item(const QString &name) const;
    bool click(const QString &name);
    bool clickItem(QQuickItem *control);
    bool type(const QString &name, const QString &text);
    QObject *popup(const QString &name) const;
    QVariantMap occurrence() const;
    bool capture(const QString &name);
    bool startConcurrentEdit();
    bool checkTimeline(const QString &date, const QString &label);
    MailBackend &m_backend;
    QQuickWindow *m_window;
    std::function<void(const QVariantMap &, const QString &)> m_finished;
    QTimer m_timer;
    QVariantMap m_result;
    QString m_eventId, m_writeError;
    QString m_profile, m_engine, m_externalError;
    QString m_originalTimelineDate, m_timelineCalendarDate, m_timelineCalendarView, m_timelineWorkspace;
    QVariantMap m_timelineDatePreferences;
    QPointer<QQuickItem> m_timelineFocus;
    QPointer<QQuickItem> m_timelineSurface;
    QVariantMap m_timelineEvent;
    QProcess m_external;
    int m_stage = 0, m_ticks = 0, m_view = 0;
    bool m_written = false, m_writeOk = false;
    bool m_externalDone = false;
    bool m_timelineSettingsOpen = false;
};
