#pragma once
#include <QQuickWindow>
#include <QObject>
#include <atomic>
#include <chrono>
#include <functional>

/// Shared developer measurement boundary. The owner must outlive the QML window,
/// as main.cpp arranges for each benchmark, so direct render callbacks stay valid.
///
/// Used by the GUI benchmarks to time input-to-pixels latency. It hooks the
/// window's afterSynchronizing, afterRendering and frameSwapped signals with
/// direct connections, so those handlers run on the render thread (or the GUI
/// thread with the basic render loop), and reports each swapped frame to the
/// callback on the observer's own (GUI) thread.
class GuiPaintObserver final : public QObject {
public:
    /// One presented frame. All times are steady-clock nanoseconds from
    /// nowNanos().
    ///
    /// - `generation`: the arm() value in effect when this frame's scene was
    ///   synchronised.
    /// - `serial`: counts synchronisations; identifies the scene state shown.
    /// - `swappedAt`: when the frame was swapped to the screen.
    /// - `synchronizedAt`: when the GUI-to-render sync finished for it.
    /// - `renderedAt`: when rendering finished, before the swap.
    /// - `callbackAt`: when the callback ran on the GUI thread.
    struct Frame {
        quint64 generation; quint64 serial; qint64 swappedAt;
        qint64 synchronizedAt; qint64 renderedAt; qint64 callbackAt;
    };
    /// Starts observing `window`. `callback` runs once per swapped frame, queued
    /// to this object's thread.
    GuiPaintObserver(QQuickWindow *window, std::function<void(Frame)> callback, QObject *parent)
        : QObject(parent), m_callback(std::move(callback)) {
        connect(window, &QQuickWindow::afterSynchronizing, this, [this] {
            m_syncCount.fetch_add(1, std::memory_order_relaxed);
            ++m_syncSerial;
            m_syncGeneration = m_requested.load(std::memory_order_acquire);
            m_synchronizedAt = nowNanos();
        }, Qt::DirectConnection);
        connect(window, &QQuickWindow::afterRendering, this, [this] {
            m_renderCount.fetch_add(1, std::memory_order_relaxed);
            m_renderedSerial = m_syncSerial;
            m_renderedGeneration = m_syncGeneration;
            m_renderedSynchronizedAt = m_synchronizedAt;
            m_renderedAt = nowNanos();
        }, Qt::DirectConnection);
        connect(window, &QQuickWindow::frameSwapped, this, [this] {
            m_swapCount.fetch_add(1, std::memory_order_relaxed);
            Frame frame{m_renderedGeneration, m_renderedSerial, nowNanos(), m_renderedSynchronizedAt, m_renderedAt, 0};
            QMetaObject::invokeMethod(this, [this, frame]() mutable { frame.callbackAt = nowNanos(); m_callback(frame); }, Qt::QueuedConnection);
        }, Qt::DirectConnection);
    }
    /// Tags frames synchronised from now on with `generation`. Call it on the
    /// GUI thread right after the change to be measured, so a frame whose
    /// `generation` matches is known to include it. Benchmarks pass 0 to
    /// mean "nothing armed".
    void arm(quint64 generation) { m_requested.store(generation, std::memory_order_release); }
    /// Totals since construction; safe to read from any thread.
    quint64 syncCount() const { return m_syncCount.load(std::memory_order_relaxed); }
    quint64 renderCount() const { return m_renderCount.load(std::memory_order_relaxed); }
    quint64 swapCount() const { return m_swapCount.load(std::memory_order_relaxed); }
    /// Monotonic time in nanoseconds (std::chrono::steady_clock), the clock
    /// for every Frame field.
    static qint64 nowNanos() {
        return std::chrono::duration_cast<std::chrono::nanoseconds>(
            std::chrono::steady_clock::now().time_since_epoch()).count();
    }
private:
    std::function<void(Frame)> m_callback;
    std::atomic<quint64> m_requested{0};
    std::atomic<quint64> m_syncCount{0}, m_renderCount{0}, m_swapCount{0};
    // Render-thread-only state, captured before the queued GUI callback.
    quint64 m_syncSerial = 0, m_syncGeneration = 0;
    quint64 m_renderedSerial = 0, m_renderedGeneration = 0;
    qint64 m_synchronizedAt = 0, m_renderedSynchronizedAt = 0, m_renderedAt = 0;
};
