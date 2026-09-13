#pragma once
#include "MailBackend.h"
#include "GuiPaintObserver.h"
#include <QJsonArray>
#include <QQuickItem>

// Developer-only native navigation measurement on a disposable synthetic profile.
class GuiInteractionBenchmark final : public QObject {
public:
    GuiInteractionBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent);
    void stopForShutdown();
    bool passed() const { return m_budgetPassed; }
private:
    enum class Phase { Starting, BetweenSamples, AwaitingModel, AwaitingPaint, Finished };
    void startWhenReady();
    void beginSample();
    void prepareLargeMailbox();
    void inspect();
    bool modelCorrect() const;
    bool visibleSnapshot(QJsonObject &snapshot) const;
    QJsonObject diagnostic() const;
    void painted(GuiPaintObserver::Frame frame);
    void recordFrame(GuiPaintObserver::Frame frame, const QString &reason, const QJsonObject &snapshot = {});
    bool click(QQuickItem *item);
    void finish(const QString &error = {});
    MailBackend &m_backend;
    QQuickWindow *m_window;
    GuiPaintObserver *m_paint;
    QString m_output, m_targetAccount, m_targetSelected, m_targetMessage;
    QStringList m_accounts;
    QString m_largeMailbox;
    qint64 m_largeMailboxCount = 0;
    QHash<QString, QString> m_restoredSelections;
    QTimer m_startTimer, m_inspectTimer, m_watchdog;
    QJsonArray m_samples[5];
    QJsonObject m_input, m_armedSnapshot;
    QJsonArray m_armEvents, m_frameEvents;
    int m_armEventsDropped = 0, m_frameEventsDropped = 0;
    int m_class = 0, m_iteration = 0;
    int m_routeCount = 4;
    quint64 m_generation = 0, m_armedGeneration = 0, m_lastSerial = 0;
    qint64 m_inputAt = 0, m_modelAt = 0, m_visibleAt = 0, m_inputUnixMs = 0;
    int m_modelInvalidations = 0, m_visibilityWaits = 0;
    Phase m_phase = Phase::Starting;
    bool m_budgetPassed = false;
    bool m_preparingLargeMailbox = false;
};
