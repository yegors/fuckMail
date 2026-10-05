#pragma once
#include "MailBackend.h"
#include "GuiPaintObserver.h"
#include <QJsonArray>
#include <QJsonObject>
#include <QQuickItem>
#include <QTimer>

// Developer-only native Calendar measurement on a disposable synthetic profile:
// actual control input to the first frame whose visible grid matches the
// requested view, date and published events, and Mail/Calendar switches to
// the first frame showing the other workspace.
class GuiCalendarBenchmark final : public QObject {
public:
    GuiCalendarBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent);
    void stopForShutdown();
    bool passed() const { return m_budgetPassed; }
private:
    enum class Phase { Starting, BetweenSamples, AwaitingModel, AwaitingPaint, Finished };
    void startWhenReady();
    void beginSample();
    void inspect();
    bool modelCorrect() const;
    bool visibleSnapshot(QJsonObject &snapshot) const;
    void painted(GuiPaintObserver::Frame frame);
    bool click(const QString &name);
    void finish(const QString &error = {});
    MailBackend &m_backend;
    QQuickWindow *m_window;
    GuiPaintObserver *m_paint;
    QString m_output, m_targetView, m_targetDate, m_targetWorkspace = "calendar";
    QTimer m_startTimer, m_inspectTimer, m_watchdog;
    QJsonArray m_samples[5];
    QJsonObject m_input, m_armedSnapshot;
    mutable QString m_snapshotMiss;
    int m_class = 0, m_iteration = 0;
    quint64 m_generation = 0, m_armedGeneration = 0, m_lastSerial = 0;
    qint64 m_inputAt = 0, m_modelAt = 0;
    int m_intermediateFrames = 0, m_modelInvalidations = 0;
    Phase m_phase = Phase::Starting;
    bool m_setup = false, m_budgetPassed = false;
};
