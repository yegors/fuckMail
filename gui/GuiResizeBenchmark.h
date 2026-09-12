#pragma once
#include "MailBackend.h"
#include "GuiPaintObserver.h"
#include <QJsonArray>
#include <QJsonObject>
#include <QPointer>
#include <QQuickItem>
#include <QTimer>

// Developer-only, disposable-profile measurement. The owner must outlive the
// QML window because GuiPaintObserver also observes the render thread.
class GuiResizeBenchmark final : public QObject {
public:
    GuiResizeBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent);
    void stopForShutdown();
    bool passed() const { return m_passed; }
private:
    enum class Phase { Waiting, Handoff, Preparing, Timing, Draining, CaptureSettle, CaptureFrame,
        ScrollInitial, ScrollAuthored, ScrollSmaller, ScrollReturned, Restoring, Finished };
    struct Offer {
        quint64 generation = 0;
        int slot = 0;
        qint64 scheduled = 0, offered = 0;
        QSize requested, observed;
        bool painted = false;
        double latency = 0;
    };
    bool eventFilter(QObject *watched, QEvent *event) override;
    void tick();
    void beginSurface();
    void beginTiming();
    void offer(int slot);
    void painted(GuiPaintObserver::Frame frame);
    void endTiming();
    void prepareCapture();
    void capture(GuiPaintObserver::Frame frame);
    void beginScrollStep(Phase phase, QSize size);
    void restoreWindow();
    QJsonObject calendarScrollState() const;
    void finish(const QString &error = {});
    QJsonObject scrollState() const;
    QJsonArray lineState(const QImage &image) const;
    QString workspace() const;
    MailBackend &m_backend;
    QQuickWindow *m_window;
    GuiPaintObserver m_observer;
    QTimer m_timer;
    QString m_output, m_captureDirectory, m_originalWorkspace;
    QRect m_originalGeometry;
    QVariant m_originalSettings;
    QVariantMap m_originalPresentation;
    QList<QPair<QPointer<QQuickItem>, QVariant>> m_originalScroll;
    QList<Offer> m_offers;
    QList<double> m_intervals, m_latencies;
    QJsonArray m_surfaces, m_captures, m_frames, m_resizeEvents, m_droppedSlots;
    QJsonObject m_surface, m_beforeScroll, m_scrollExercise;
    QJsonArray m_firstLines;
    qint64 m_began = 0, m_phaseBegan = 0, m_traceBegan = 0, m_lastSwap = 0;
    quint64 m_generation = 0, m_latestGeneration = 0, m_lastSerial = 0, m_updates = 0;
    QSize m_captureSize;
    double m_scrollTarget = 0, m_scrollHour = 0;
    int m_surfaceIndex = 0, m_lastSlot = -1, m_captureIndex = 0;
    Phase m_phase = Phase::Waiting;
    bool m_finished = false, m_passed = false, m_savedState = false, m_captureQueued = false, m_scrollFrameSeen = false;
};
