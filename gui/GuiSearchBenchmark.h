#pragma once
#include "MailBackend.h"
#include <QQuickWindow>
#include <QQuickItem>
#include <QJsonArray>
#include <atomic>

// Developer-only measurement against a retained generated 100k fixture.
class GuiSearchBenchmark final : public QObject {
public:
    GuiSearchBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent);
    void stopForShutdown();
private:
    bool eventFilter(QObject *watched, QEvent *event) override;
    enum class Phase { Preparing, ResetQuery, TargetQuery, ResetPaint, TargetPaint };
    void beginSample();
    void inputQuery(const QString &query, Phase phase);
    void resultsReady();
    void painted(quint64 generation, quint64 serial, qint64 swappedAt);
    QString validateTarget(const QVariantList &rows);
    void finish(const QString &error = {});
    MailBackend &m_backend;
    QQuickWindow *m_window;
    QQuickItem *m_search = nullptr;
    QString m_output, m_expected, m_resetNonce;
    QVariantList m_armedRows;
    QTimer m_startTimer, m_phaseWatchdog;
    QList<double> m_samples[4];
    QStringList m_expectedIds[4];
    int m_modelReplacements[4] = {}, m_resetPaints[4] = {};
    int m_class = 0, m_iteration = 0;
    quint64 m_generation = 0, m_armedGeneration = 0, m_lastPaintSerial = 0;
    quint64 m_lastObservedGeneration = 0, m_lastObservedSerial = 0;
    quint64 m_updateRequests = 0;
    qint64 m_inputAt = 0;
    Phase m_phase = Phase::Preparing;
    bool m_loadingSeen = false, m_modelChanged = false, m_finished = false;
    // Only this handoff is shared by GUI and render threads. The fields below
    // it are touched exclusively in the render-thread direct signal handlers.
    std::atomic<quint64> m_requestedGeneration{0};
    std::atomic<quint64> m_syncCount{0}, m_renderCount{0}, m_swapCount{0};
    quint64 m_syncSerial = 0, m_syncGeneration = 0;
    quint64 m_renderedSerial = 0, m_renderedGeneration = 0;
};
