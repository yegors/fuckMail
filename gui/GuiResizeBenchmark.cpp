#include "GuiResizeBenchmark.h"
#include <QColor>
#include <QCoreApplication>
#include <QCursor>
#include <QDateTime>
#include <QDebug>
#include <QDir>
#include <QFileInfo>
#include <QImage>
#include <QJsonDocument>
#include <QResizeEvent>
#include <QSaveFile>
#include <QScreen>
#include <QSGRendererInterface>
#include <QSysInfo>
#include <algorithm>
#include <cmath>

namespace {
constexpr int slotCount = 160;
constexpr qint64 slotNanos = 16000000;
constexpr int captureCount = 15; // settled A/B/A, then twelve first-matched-frame readbacks
QJsonObject sizeJson(QSize size) { return {{"width", size.width()}, {"height", size.height()}}; }
QJsonArray rectJson(QRectF rect) { return {rect.x(), rect.y(), rect.width(), rect.height()}; }
QJsonObject distribution(QList<double> values) {
    if (values.isEmpty()) return {{"count", 0}};
    std::sort(values.begin(), values.end());
    auto percentile = [&](double p) { return values[std::max(0, int(std::ceil(values.size() * p)) - 1)]; };
    return {{"count", values.size()}, {"p50_ms", percentile(.50)}, {"p95_ms", percentile(.95)},
        {"p99_ms", percentile(.99)}, {"max_ms", values.last()}};
}
}

GuiResizeBenchmark::GuiResizeBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window),
      m_observer(window, [this](GuiPaintObserver::Frame frame) { painted(frame); }, this), m_output(std::move(output)) {
    m_began = m_phaseBegan = GuiPaintObserver::nowNanos();
    m_captureDirectory = QFileInfo(m_output).absolutePath() + "/" + QFileInfo(m_output).completeBaseName() + "-captures";
    m_window->installEventFilter(this);
    m_window->raise(); m_window->requestActivate();
    m_timer.setTimerType(Qt::PreciseTimer); m_timer.setInterval(16);
    connect(&m_timer, &QTimer::timeout, this, &GuiResizeBenchmark::tick);
    connect(&m_backend, &MailBackend::errorChanged, this, [this] {
        if (!m_finished && !m_backend.error().isEmpty()) finish(m_backend.error());
    });
    m_timer.start();
}

QString GuiResizeBenchmark::workspace() const { return m_surfaceIndex == 0 ? "mail" : "calendar"; }

bool GuiResizeBenchmark::eventFilter(QObject *watched, QEvent *event) {
    if (watched == m_window && !m_finished) {
        if (event->type() == QEvent::UpdateRequest) ++m_updates;
        if (event->type() == QEvent::Resize && (m_phase == Phase::Timing || m_phase == Phase::Draining)) {
            const auto *resize = static_cast<QResizeEvent *>(event);
            m_resizeEvents.append(QJsonObject{{"generation", qint64(m_latestGeneration)},
                {"offset_ms", (GuiPaintObserver::nowNanos() - m_traceBegan) / 1e6},
                {"old_size", sizeJson(resize->oldSize())}, {"size", sizeJson(resize->size())}});
        }
    }
    return QObject::eventFilter(watched, event);
}

void GuiResizeBenchmark::tick() {
    if (m_finished) return;
    const auto now = GuiPaintObserver::nowNanos();
    const qint64 phaseLimit = m_phase == Phase::Waiting ? 45000000000LL : 15000000000LL;
    if (now - m_began > 120000000000LL || now - m_phaseBegan > phaseLimit) {
        finish(QString("Resize benchmark stalled: phase=%1 surface=%2 capture=%3 active=%4 exposed=%5 size=%6x%7 swaps=%8")
            .arg(int(m_phase)).arg(workspace()).arg(m_captureIndex).arg(m_window->isActive()).arg(m_window->isExposed())
            .arg(m_window->width()).arg(m_window->height()).arg(m_observer.swapCount())); return;
    }
    if (m_phase == Phase::Waiting) {
        if (!m_window->isActive() || !m_window->isExposed() || !m_backend.connected() || m_backend.loading()
            || !m_backend.calendarReady() || !m_backend.timelineReady() || m_backend.preferences().isEmpty()
            || m_backend.conversation().isEmpty() || m_backend.draft().isEmpty()) return;
        if (QFileInfo::exists(m_output) || QFileInfo::exists(m_captureDirectory)) {
            finish("Resize output or capture directory already exists; refusing to overwrite evidence."); return;
        }
        if (!QDir().mkpath(m_captureDirectory)) { finish("Cannot create the designated resize capture directory."); return; }
        m_originalGeometry = m_window->geometry();
        m_originalWorkspace = m_window->property("workspace").toString();
        m_originalSettings = m_window->property("settingsOpen");
        m_originalPresentation = m_backend.presentation();
        for (const auto &name : {"conversationList", "reader", "calendarGrid"})
            if (auto *item = m_window->findChild<QQuickItem *>(name)) m_originalScroll.append({item, item->property("contentY")});
        m_savedState = true;
        m_window->setProperty("settingsOpen", false);
        qInfo("Native resize benchmark prepared");
        m_phase = Phase::Handoff; m_phaseBegan = now; return;
    }
    if (m_phase == Phase::Handoff && now - m_phaseBegan >= 500000000) { beginSurface(); return; }
    if (m_phase == Phase::Preparing && now - m_phaseBegan >= 400000000) { beginTiming(); return; }
    if (m_phase == Phase::Timing) {
        const int due = int((now - m_traceBegan) / slotNanos);
        const int target = std::min(due, slotCount - 1);
        if (target > m_lastSlot) {
            for (int skipped = m_lastSlot + 1; skipped < target; ++skipped) m_droppedSlots.append(skipped);
            offer(target);
        }
        if (due >= slotCount) { m_phase = Phase::Draining; m_phaseBegan = now; }
        return;
    }
    if (m_phase == Phase::Draining && (now - m_phaseBegan >= 250000000 || (!m_offers.isEmpty() && m_offers.last().painted))) {
        endTiming(); return;
    }
    if (m_phase == Phase::CaptureSettle && now - m_phaseBegan >= (m_captureIndex < 3 ? 300000000 : 0)) {
        m_phase = Phase::CaptureFrame; m_phaseBegan = now;
        m_latestGeneration = ++m_generation; m_observer.arm(m_latestGeneration); m_window->update(); return;
    }
    if ((m_phase == Phase::ScrollInitial || m_phase == Phase::ScrollAuthored
        || m_phase == Phase::ScrollSmaller || m_phase == Phase::ScrollReturned)
        && m_scrollFrameSeen && now - m_phaseBegan >= 300000000) {
        const auto state = calendarScrollState();
        if (!state.value("visible").toBool() || state.value("hour_height").toDouble() <= 0) {
            finish("The native calendar grid was unavailable for the separate scroll-retention exercise."); return;
        }
        if (m_phase == Phase::ScrollInitial) {
            m_scrollExercise["initial_A"] = state;
            const double maximum = state.value("maximum_scroll").toDouble();
            m_scrollTarget = std::min(48.0, maximum);
            if (m_scrollTarget <= 1 / m_window->effectiveDevicePixelRatio()) {
                finish("The scroll-retention exercise requires a meaningful nonzero native calendar scroll range."); return;
            }
            m_scrollHour = m_scrollTarget / state.value("hour_height").toDouble();
            m_scrollExercise["requested_content_y"] = m_scrollTarget;
            m_scrollExercise["expected_top_hour"] = m_scrollHour;
            auto *grid = m_window->findChild<QQuickItem *>("calendarGrid");
            if (!grid || !grid->setProperty("contentY", m_scrollTarget)) {
                finish("Could not set the actual calendarGrid contentY for the scroll exercise."); return;
            }
            beginScrollStep(Phase::ScrollAuthored, QSize(1280, 720)); return;
        }
        const QString key = m_phase == Phase::ScrollAuthored ? "authored_A" : m_phase == Phase::ScrollSmaller ? "resized_B" : "returned_A";
        m_scrollExercise[key] = state;
        const double expectedY = m_scrollHour * state.value("hour_height").toDouble();
        const double errorPixels = std::abs(state.value("content_y").toDouble() - expectedY) * m_window->effectiveDevicePixelRatio();
        m_scrollExercise[key + "_error_device_pixels"] = errorPixels;
        if (errorPixels > 1.00001) {
            m_scrollExercise["passed"] = false;
            finish(QString("Calendar scroll time anchor changed at %1: expected contentY=%2 actual=%3 (%4 physical pixels).")
                .arg(key).arg(expectedY).arg(state.value("content_y").toDouble()).arg(errorPixels)); return;
        }
        if (m_phase == Phase::ScrollAuthored) { beginScrollStep(Phase::ScrollSmaller, QSize(1160, 700)); return; }
        if (m_phase == Phase::ScrollSmaller) { beginScrollStep(Phase::ScrollReturned, QSize(1280, 720)); return; }
        m_scrollExercise["passed"] = true; restoreWindow(); return;
    }
    if (m_phase == Phase::Restoring && now - m_phaseBegan >= 400000000) {
        for (const auto &saved : m_originalScroll) if (saved.first) saved.first->setProperty("contentY", saved.second);
        for (const auto &key : {"splitWidth", "listY", "readerY"})
            if (m_originalPresentation.contains(key)) m_backend.remember(key, m_originalPresentation.value(key));
        finish();
    }
}

void GuiResizeBenchmark::beginSurface() {
    m_observer.arm(0); m_window->setProperty("workspace", workspace()); m_window->resize(1280, 720);
    m_surface = {{"workspace", workspace()}, {"calendar_view", m_backend.calendarView()},
        {"account", m_backend.account()}, {"selected_id", m_backend.selectedId()}};
    m_phase = Phase::Preparing; m_phaseBegan = GuiPaintObserver::nowNanos();
}

void GuiResizeBenchmark::beginTiming() {
    m_offers.clear(); m_frames = {}; m_resizeEvents = {}; m_droppedSlots = {};
    m_latencies.clear(); m_intervals.clear(); m_lastSwap = 0; m_lastSlot = -1;
    m_beforeScroll = scrollState();
    m_traceBegan = m_phaseBegan = GuiPaintObserver::nowNanos(); m_phase = Phase::Timing;
    offer(0);
}

void GuiResizeBenchmark::offer(int slot) {
    const double fraction = slot < 80 ? slot / 79.0 : (159 - slot) / 79.0;
    Offer value; value.generation = ++m_generation; value.slot = slot;
    value.scheduled = m_traceBegan + slot * slotNanos; value.offered = GuiPaintObserver::nowNanos();
    value.requested = QSize(1000 + qRound(440 * fraction), 650 + qRound(250 * fraction));
    m_latestGeneration = value.generation; m_observer.arm(0);
    m_window->resize(value.requested); value.observed = m_window->size();
    m_offers.append(value); m_lastSlot = slot;
    m_observer.arm(value.generation); m_window->update();
}

void GuiResizeBenchmark::painted(GuiPaintObserver::Frame frame) {
    if (m_finished || frame.serial <= m_lastSerial) return;
    m_lastSerial = frame.serial;
    if (m_phase == Phase::Timing || m_phase == Phase::Draining) {
        if (frame.swappedAt < m_traceBegan) return;
        const bool latest = !m_offers.isEmpty() && frame.generation == m_latestGeneration
            && m_window->size() == m_offers.last().observed && m_window->size() == m_offers.last().requested;
        if (m_lastSwap) m_intervals.append((frame.swappedAt - m_lastSwap) / 1e6);
        m_lastSwap = frame.swappedAt;
        m_frames.append(QJsonObject{{"generation", qint64(frame.generation)}, {"serial", qint64(frame.serial)},
            {"swap_offset_ms", (frame.swappedAt - m_traceBegan) / 1e6},
            {"callback_delay_ms", (GuiPaintObserver::nowNanos() - frame.swappedAt) / 1e6},
            {"latest_geometry_validated", latest}, {"gui_observed_size", sizeJson(m_window->size())},
            {"dpr", m_window->effectiveDevicePixelRatio()}});
        if (latest && !m_offers.last().painted && frame.swappedAt >= m_offers.last().offered) {
            auto &value = m_offers.last(); value.painted = true; value.latency = (frame.swappedAt - value.offered) / 1e6;
            m_latencies.append(value.latency);
        }
    } else if (m_phase == Phase::CaptureFrame && !m_captureQueued && frame.generation == m_latestGeneration
        && m_window->size() == m_captureSize) {
        m_captureQueued = true;
        QTimer::singleShot(0, this, [this, frame] {
            if (!m_finished && m_phase == Phase::CaptureFrame && frame.generation == m_latestGeneration) capture(frame);
        });
    } else if ((m_phase == Phase::ScrollInitial || m_phase == Phase::ScrollAuthored
        || m_phase == Phase::ScrollSmaller || m_phase == Phase::ScrollReturned)
        && frame.generation == m_latestGeneration && m_window->size() == m_captureSize) {
        m_scrollFrameSeen = true;
    }
}

void GuiResizeBenchmark::endTiming() {
    m_observer.arm(0);
    QJsonArray offers;
    int unpainted = 0;
    for (const auto &value : m_offers) {
        if (!value.painted) ++unpainted;
        QJsonObject row{{"slot", value.slot}, {"generation", qint64(value.generation)},
            {"scheduled_offset_ms", (value.scheduled - m_traceBegan) / 1e6},
            {"offered_offset_ms", (value.offered - m_traceBegan) / 1e6}, {"offer_lateness_ms", (value.offered - value.scheduled) / 1e6},
            {"requested_size", sizeJson(value.requested)}, {"observed_size", sizeJson(value.observed)},
            {"accepted_geometry_frame", value.painted}};
        if (value.painted) row["offer_to_swap_ms"] = value.latency;
        offers.append(row);
    }
    m_surface["scheduled_slots"] = slotCount; m_surface["interval_ms"] = 16;
    m_surface["offered_slots"] = m_offers.size(); m_surface["dropped_timer_slot_count"] = m_droppedSlots.size();
    m_surface["resize_event_count"] = m_resizeEvents.size(); m_surface["swapped_frames"] = m_frames.size();
    m_surface["validated_geometry_frames"] = m_latencies.size();
    m_surface["offers"] = offers; m_surface["dropped_timer_slots"] = m_droppedSlots;
    m_surface["offers_without_accepted_geometry_frame"] = unpainted;
    m_surface["resize_events"] = m_resizeEvents; m_surface["frames"] = m_frames;
    m_surface["offer_to_swap"] = distribution(m_latencies); m_surface["frame_intervals"] = distribution(m_intervals);
    m_surface["scroll_before_timing"] = m_beforeScroll; m_surface["scroll_after_timing"] = scrollState();
    m_surface["trace_elapsed_ms"] = (GuiPaintObserver::nowNanos() - m_traceBegan) / 1e6;
    m_surface["tail_without_swap_ms"] = (GuiPaintObserver::nowNanos() - (m_lastSwap ? m_lastSwap : m_traceBegan)) / 1e6;
    if (!m_frames.isEmpty()) m_surface["first_swap_offset_ms"] = m_frames.first().toObject()["swap_offset_ms"];
    m_surfaces.append(m_surface);
    if (m_latencies.size() < 2) { finish("Fewer than two generation/geometry-validated resize frames were observed."); return; }
    m_captureIndex = 0; m_firstLines = {}; prepareCapture();
}

void GuiResizeBenchmark::prepareCapture() {
    m_observer.arm(0); m_captureQueued = false;
    if (m_captureIndex >= captureCount) {
        if (++m_surfaceIndex < 2) { beginSurface(); return; }
        m_scrollExercise = {{"passed", false}, {"tolerance_device_pixels", 1},
            {"boundary", "Separate correctness exercise after both timing traces and all30 PNGs: actual native contentY assignment, then settled generation-matched A1280x720/B1160x700/A1280x720 frames. No retainedTopHour or private QML state is forced."}};
        beginScrollStep(Phase::ScrollInitial, QSize(1280, 720)); return;
    }
    m_captureSize = m_captureIndex < 3 ? (m_captureIndex == 1 ? QSize(1160, 700) : QSize(1280, 720))
        : QSize(1274 + m_captureIndex - 3, 720 + (m_captureIndex - 3) % 4);
    m_window->resize(m_captureSize);
    m_phase = Phase::CaptureSettle; m_phaseBegan = GuiPaintObserver::nowNanos();
}

void GuiResizeBenchmark::beginScrollStep(Phase phase, QSize size) {
    m_observer.arm(0); m_scrollFrameSeen = false; m_captureSize = size;
    m_phase = phase; m_phaseBegan = GuiPaintObserver::nowNanos();
    m_window->resize(size);
    m_latestGeneration = ++m_generation; m_observer.arm(m_latestGeneration); m_window->update();
}

QJsonObject GuiResizeBenchmark::calendarScrollState() const {
    auto state = scrollState().value("calendarGrid").toObject();
    auto *calendar = m_window->findChild<QQuickItem *>("calendarWorkspace");
    const double hourHeight = calendar ? calendar->property("hourHeight").toDouble() : 0;
    state["hour_height"] = hourHeight;
    state["top_hour"] = hourHeight > 0 ? state.value("content_y").toDouble() / hourHeight : 0;
    state["maximum_scroll"] = std::max(0.0, state.value("content_height").toDouble() - state.value("height").toDouble());
    state["logical_window_size"] = sizeJson(m_window->size()); state["dpr"] = m_window->effectiveDevicePixelRatio();
    return state;
}

void GuiResizeBenchmark::restoreWindow() {
    m_observer.arm(0); m_window->setProperty("workspace", m_originalWorkspace);
    m_window->setProperty("settingsOpen", m_originalSettings); m_window->setGeometry(m_originalGeometry);
    m_phase = Phase::Restoring; m_phaseBegan = GuiPaintObserver::nowNanos();
}

QJsonObject GuiResizeBenchmark::scrollState() const {
    QJsonObject result;
    for (const auto &name : {"conversationList", "reader", "calendarGrid"}) {
        if (auto *item = m_window->findChild<QQuickItem *>(name)) {
            result[name] = QJsonObject{{"visible", item->isVisible()}, {"content_y", item->property("contentY").toDouble()},
                {"origin_y", item->property("originY").toDouble()}, {"content_height", item->property("contentHeight").toDouble()},
                {"height", item->height()}};
        }
    }
    return result;
}

QJsonArray GuiResizeBenchmark::lineState(const QImage &image) const {
    QJsonArray lines;
    const double dpr = m_window->effectiveDevicePixelRatio();
    const QRectF viewport(QPointF(), QSizeF(m_window->size()));
    std::function<void(QQuickItem *, const QString &)> visit = [&](QQuickItem *item, const QString &path) {
        if (!item->isVisible() || item->opacity() <= 0 || lines.size() >= 256) return;
        const QRectF rect = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
        const QVariant fillValue = item->property("color");
        // Do not read Rectangle.border here: that getter lazily creates a pen.
        // Inspect only already-existing public QObject children, without Qt private headers.
        QObject *border = nullptr;
        for (auto *child : item->children()) if (child->inherits("QQuickPen")) { border = child; break; }
        const bool hasBorder = border && border->property("width").toDouble() > 0;
        const bool thin = (rect.width() <= 2.5 && rect.height() >= 14) || (rect.height() <= 2.5 && rect.width() >= 20);
        if (fillValue.canConvert<QColor>() && rect.intersects(viewport) && (thin || hasBorder)) {
            const QColor fill = fillValue.value<QColor>();
            QJsonArray samples;
            const auto sample = [&](QPointF p) {
                const int x = qRound(p.x() * dpr), y = qRound(p.y() * dpr);
                if (image.rect().contains(x, y)) samples.append(QJsonObject{{"x", x}, {"y", y}, {"argb", image.pixelColor(x, y).name(QColor::HexArgb)}});
            };
            for (double position : {.2, .5, .8}) {
                for (int offset = -2; offset <= 2; ++offset) {
                    if (thin && rect.width() <= 2.5) sample({rect.center().x() + offset / dpr, rect.y() + rect.height() * position});
                    else if (thin) sample({rect.x() + rect.width() * position, rect.center().y() + offset / dpr});
                }
            }
            if (hasBorder) for (const auto &point : {QPointF(rect.left(), rect.center().y()), QPointF(rect.right(), rect.center().y()),
                QPointF(rect.center().x(), rect.top()), QPointF(rect.center().x(), rect.bottom())}) sample(point);
            double effectiveOpacity = 1;
            QRectF unclipped = rect.intersected(viewport);
            for (auto *ancestor = item; ancestor; ancestor = ancestor->parentItem()) {
                effectiveOpacity *= ancestor->opacity();
                if (ancestor->clip()) unclipped = unclipped.intersected(ancestor->mapRectToScene(QRectF(0, 0, ancestor->width(), ancestor->height())));
            }
            QJsonArray states;
            const auto stateOf = [&](QObject *object) {
                QJsonObject state{{"class", object->metaObject()->className()}, {"object_name", object->objectName()}};
                for (const auto &key : {"hovered", "containsMouse", "pressed", "activeFocus"}) {
                    const auto value = object->property(key);
                    if (value.isValid()) state[key] = value.toBool();
                }
                if (state.size() > 2) states.append(state);
            };
            for (auto *ancestor = item; ancestor; ancestor = ancestor->parentItem()) stateOf(ancestor);
            for (auto *child : item->children()) if (child->inherits("QQuickSplitHandle")) stateOf(child);
            QJsonObject row{{"path", path}, {"object_name", item->objectName()}, {"class", item->metaObject()->className()},
                {"scene_rect", rectJson(rect)}, {"device_rect", rectJson(QRectF(rect.topLeft() * dpr, rect.size() * dpr))},
                {"fill_argb", fill.name(QColor::HexArgb)}, {"opacity", item->opacity()},
                {"effective_opacity", effectiveOpacity}, {"unclipped_scene_rect", rectJson(unclipped)},
                {"antialiasing", item->antialiasing()}, {"interaction_states", states}, {"samples", samples}};
            if (hasBorder) { row["border_argb"] = border->property("color").value<QColor>().name(QColor::HexArgb);
                row["border_width"] = border->property("width").toDouble(); row["border_pixel_aligned"] = border->property("pixelAligned").toBool(); }
            lines.append(row);
        }
        const auto children = item->childItems();
        for (int i = 0; i < children.size(); ++i) visit(children[i], path + "/" + QString::number(i));
    };
    visit(m_window->contentItem(), "content");
    return lines;
}

void GuiResizeBenchmark::capture(GuiPaintObserver::Frame frame) {
    if (m_window->size() != m_captureSize) { finish("Window geometry changed before the separate image readback."); return; }
    const auto began = GuiPaintObserver::nowNanos();
    const auto image = m_window->grabWindow();
    const QString label = m_captureIndex < 3 ? QString("settled-%1").arg(m_captureIndex)
        : QString("resize-%1").arg(m_captureIndex - 3, 2, 10, QLatin1Char('0'));
    const auto path = QDir(m_captureDirectory).filePath(workspace() + "-" + label + ".png");
    if (image.isNull() || QFileInfo::exists(path) || !image.save(path)) { finish("Cannot retain a separate resize PNG readback."); return; }
    const auto lines = lineState(image);
    const QPoint cursor = m_window->mapFromGlobal(QCursor::pos());
    QJsonObject row{{"workspace", workspace()}, {"kind", m_captureIndex < 3 ? "settled_A_B_A" : "successive_resize_readback"},
        {"sequence", m_captureIndex}, {"path", path}, {"logical_size", sizeJson(m_window->size())}, {"pixel_size", sizeJson(image.size())},
        {"dpr", m_window->effectiveDevicePixelRatio()}, {"generation", qint64(frame.generation)}, {"serial", qint64(frame.serial)},
        {"readback_after_matched_swap_ms", (began - frame.swappedAt) / 1e6},
        {"readback_and_save_ms", (GuiPaintObserver::nowNanos() - began) / 1e6},
        {"cursor_window_position", QJsonArray{cursor.x(), cursor.y()}}, {"lines", lines}, {"scroll", scrollState()}};
    if (m_captureIndex == 0) m_firstLines = lines;
    if (m_captureIndex == 2) row["same_geometry_line_metadata_and_samples_equal"] = lines == m_firstLines;
    m_captures.append(row); ++m_captureIndex; prepareCapture();
}

void GuiResizeBenchmark::stopForShutdown() {
    m_finished = true; m_timer.stop(); m_observer.arm(0);
}

void GuiResizeBenchmark::finish(const QString &error) {
    if (m_finished) return;
    stopForShutdown();
    if (m_savedState) {
        m_window->setProperty("workspace", m_originalWorkspace); m_window->setProperty("settingsOpen", m_originalSettings);
        m_window->setGeometry(m_originalGeometry);
        for (const auto &saved : m_originalScroll) if (saved.first) saved.first->setProperty("contentY", saved.second);
    }
    m_passed = error.isEmpty() && m_surfaces.size() == 2 && m_captures.size() == captureCount * 2
        && m_scrollExercise.value("passed").toBool();
    const QJsonObject report{{"version", 1}, {"result", m_passed ? "measured" : "failed"}, {"error", error},
        {"measurement_complete", m_passed}, {"host", QSysInfo::prettyProductName()}, {"architecture", QSysInfo::currentCpuArchitecture()},
        {"qt_version", QT_VERSION_STR}, {"renderer_api", int(m_window->rendererInterface()->graphicsApi())},
        {"dpr", m_window->effectiveDevicePixelRatio()}, {"screen_refresh_hz", m_window->screen() ? m_window->screen()->refreshRate() : 0},
        {"windows_vblank_thread_disabled", qEnvironmentVariableIntValue("QT_D3D_NO_VBLANK_THREAD") != 0},
        {"theme", m_backend.preferences().value("theme").toString()}, {"text_size", m_backend.preferences().value("text_size").toString()},
        {"elapsed_ms", (GuiPaintObserver::nowNanos() - m_began) / 1e6}, {"synchronizations", qint64(m_observer.syncCount())},
        {"renders", qint64(m_observer.renderCount())}, {"swaps", qint64(m_observer.swapCount())}, {"update_requests", qint64(m_updates)},
        {"surfaces", m_surfaces}, {"captures", m_captures}, {"calendar_scroll_exercise", m_scrollExercise}, {"restored_scroll", scrollState()},
        {"last_phase", int(m_phase)}, {"last_surface", workspace()},
        {"partial_frames_on_failure", error.isEmpty() ? QJsonArray{} : m_frames},
        {"partial_resize_events_on_failure", error.isEmpty() ? QJsonArray{} : m_resizeEvents},
        {"boundary", "160 elapsed-clock 16ms offered resize slots per workspace; late timer slots are skipped and recorded, never burst or gated on completed frames. Latency accepts only the latest generation captured through sync/render/swap whose requested and GUI-observed size still match. All swap intervals are retained, including frames rejected for per-offer latency."},
        {"limits", "Programmatic native QWindow resizing, not physical Windows edge-drag input. frameSwapped means queued presentation, not monitor scanout. Superseded offers without a validated GUI callback are reported, not assumed unrendered. Normal polling and presentation persistence remain enabled. Each theme is a separate driver-prepared run. PNG readbacks are a separate slower trace: grabWindow can render again after the matched swap; these images do not establish every transient displayed frame. No invented resize latency budget or automatic visual-stability pass."}};
    // Refuse to overwrite an existing report even on a setup failure.
    if (QFileInfo::exists(m_output)) { qCritical() << "Resize output exists:" << m_output; QCoreApplication::exit(8); return; }
    QSaveFile output(m_output);
    if (!output.open(QIODevice::WriteOnly) || output.write(QJsonDocument(report).toJson()) < 0 || !output.commit()) {
        qCritical() << "Cannot write resize measurement report:" << m_output; QCoreApplication::exit(8); return;
    }
    if (!m_passed) { qCritical() << "Resize measurement failed:" << error; QCoreApplication::exit(7); return; }
    qInfo() << "Native resize benchmark completed:" << m_output;
    m_backend.shutdown();
}
