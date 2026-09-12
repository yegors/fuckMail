#pragma once
#include "MailBackend.h"
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QProcess>
#include <QQuickWindow>
#include <QTimer>
#include <functional>

// Native composition walkthrough restricted to the disposable synthetic profile.
class GuiCompositionSelfTest final : public QObject {
public:
    GuiCompositionSelfTest(MailBackend &backend, QQuickWindow *window,
        QString profile, QString engine,
        std::function<void(const QVariantMap &, const QString &)> finished, QObject *parent);
    static bool verifyRestartProjection(MailBackend &backend, QQuickWindow *window,
        const QString &expectedPath, QString &error);
    static bool verifyRetained(const QString &profile, const QString &engine,
        const QVariantMap &expected, QString &error);
private:
    void step();
    void finish(const QString &error = {});
    QQuickItem *item(const QString &name) const;
    bool click(const QString &name);
    bool clickItem(QQuickItem *control);
    bool type(const QString &name, const QString &text);
    bool recipient(const QString &field, const QString &text);
    bool saved() const;
    void cli(const QStringList &arguments, std::function<void(const QJsonObject &)> callback, const QString &account = "windscribe");
    QJsonObject cliWhilePaused(const QString &account, const QString &method, const QJsonObject &params);
    bool retainDraft(const QJsonObject &draft);
    bool startRestart();
    void restore();
    MailBackend &m_backend;
    QQuickWindow *m_window;
    QString m_profile, m_engine;
    std::function<void(const QVariantMap &, const QString &)> m_finished;
    QTimer m_timer, m_processTimeout;
    QProcess m_cli, m_child;
    QVariantMap m_originalDraft, m_originalView, m_originalFilters;
    QString m_originalAccount, m_originalSelected, m_originalDraftId, m_originalQuery, m_originalFolder, m_originalCategory, m_originalWorkspace;
    QPointer<QQuickItem> m_originalFocus;
    bool m_originalSettings = false;
    QString m_draftId, m_body, m_subject, m_restartPath, m_childPid;
    QJsonArray m_drafts, m_captures;
    QJsonObject m_sourceBefore, m_captureOperation, m_invalidSaved, m_frozenLocal, m_frozenStored;
    bool m_waiting = false, m_childDone = false;
    int m_stage = 0, m_ticks = 0, m_lastStage = -1, m_mode = 0, m_afterCapture = 9;
};
