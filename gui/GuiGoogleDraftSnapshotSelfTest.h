#pragma once
#include <QObject>
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QTimer>
class MailBackend;
class QQuickWindow;
class QQuickItem;

// Separate cached-import walkthrough; never starts a provider synchronization job.
class GuiGoogleDraftSnapshotSelfTest final : public QObject {
public:
    GuiGoogleDraftSnapshotSelfTest(MailBackend &, QQuickWindow *, const QString &expectation,
                                  const QString &report, QObject *parent = nullptr);
    bool passed() const { return m_passed; }
    static bool verifyReadOnlyControls(MailBackend &, QQuickItem *, QString &error);
private:
    void tick();
    void finish(const QString &error = {});
    bool click(const QString &name);
    bool verifyProjection(QString &error) const;
    bool verifyExport(const QString &attachmentId, const QString &hash, QString &error);
    MailBackend &m_backend;
    QPointer<QQuickWindow> m_window;
    QTimer m_timer, m_watchdog;
    QString m_report, m_draftId;
    QJsonArray m_accounts, m_observed, m_exports;
    int m_stage = 0, m_visit = 0, m_attachment = 0, m_pages = 0;
    bool m_passed = false, m_finished = false;
};
