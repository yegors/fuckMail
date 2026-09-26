// Developer benchmark for `fuckmail-gui --benchmark-calendar-output <report.json>
// --profile <dir>`: times Calendar view and date input to the first frame whose
// grid matches, and writes a JSON report. Not run by CTest.
#include "GuiCalendarBenchmark.h"
#include "GuiBenchmarkSupport.h"
#include <QCoreApplication>
#include <QDate>
#include <QDebug>
#include <QJsonDocument>
#include <QMouseEvent>
#include <QSaveFile>
#include <QSGRendererInterface>
#include <QSysInfo>
#include <algorithm>

using namespace GuiBenchmarkSupport;
namespace {
const QStringList labels{"view_switch", "month_day_click", "week_step", "month_step"};
QList<QQuickItem *> ordered(QQuickItem *parent, const QString &prefix) {
    QList<QQuickItem *> items;
    namedItems(parent, prefix, items);
    std::sort(items.begin(), items.end(), [](auto a, auto b) {
        const auto ap = a->mapToScene(QPointF{}), bp = b->mapToScene(QPointF{});
        return ap.y() != bp.y() ? ap.y() < bp.y() : ap.x() < bp.x();
    });
    return items;
}
QStringList names(const QList<QQuickItem *> &items) {
    QStringList result;
    for (auto item : items) result.append(item->objectName());
    return result;
}
// The same identity the QML delegates put in their objectNames.
QString eventName(const QVariantMap &row) {
    const auto id = row.value("event_id").toString();
    return id.isEmpty() ? row.value("id").toString() : id;
}
}

GuiCalendarBenchmark::GuiCalendarBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window),
      m_paint(new GuiPaintObserver(window, [this](auto frame) { painted(frame); }, this)), m_output(std::move(output)) {
    m_window->raise(); m_window->requestActivate();
    m_startTimer.setInterval(50);
    connect(&m_startTimer, &QTimer::timeout, this, &GuiCalendarBenchmark::startWhenReady);
    m_inspectTimer.setInterval(5); m_inspectTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_inspectTimer, &QTimer::timeout, this, &GuiCalendarBenchmark::inspect);
    m_watchdog.setSingleShot(true);
    connect(&m_watchdog, &QTimer::timeout, this, [this] {
        finish(QString("Calendar input stalled: phase=%1 class=%2 sample=%3 view=%4 date=%5 target=%6/%7 ready=%8")
            .arg(int(m_phase)).arg(m_class).arg(m_iteration).arg(m_backend.calendarView(), m_backend.calendarDate(), m_targetView, m_targetDate)
            .arg(m_backend.calendarReady()));
    });
    for (auto signal : {&MailBackend::eventsChanged, &MailBackend::calendarStatusChanged})
        connect(&m_backend, signal, this, &GuiCalendarBenchmark::inspect);
    connect(&m_backend, &MailBackend::errorChanged, this, [this] {
        if (!m_backend.error().isEmpty()) finish(m_backend.error());
    });
    QTimer::singleShot(600000, this, [this] { if (m_phase != Phase::Finished) finish("The calendar benchmark exceeded its total watchdog."); });
    m_startTimer.start(); m_watchdog.start(45000);
}
void GuiCalendarBenchmark::stopForShutdown() {
    m_phase = Phase::Finished; m_startTimer.stop(); m_inspectTimer.stop(); m_watchdog.stop(); m_paint->arm(0);
}
void GuiCalendarBenchmark::startWhenReady() {
    if (findItem(m_window->contentItem(), "startupOverlay")) return;
    if (m_phase != Phase::Starting || !m_window->isActive() || !m_window->isExposed() || !m_backend.connected()) return;
    // Setup is not measured: open this week in Calendar and let its neighbours load.
    if (m_window->property("workspace").toString() != "calendar" || m_window->property("settingsOpen").toBool()) {
        m_window->setProperty("settingsOpen", false); m_window->setProperty("workspace", "calendar"); return;
    }
    m_targetView = "week"; m_targetDate = m_backend.todayDate();
    if (m_backend.calendarView() != m_targetView) { m_backend.setCalendarView(m_targetView); return; }
    if (m_backend.calendarDate() != m_targetDate) { m_backend.setCalendarDate(m_targetDate); return; }
    QJsonObject snapshot;
    if (m_backend.events().isEmpty() || !visibleSnapshot(snapshot)) return;
    m_startTimer.stop(); m_watchdog.stop(); m_phase = Phase::BetweenSamples;
    qInfo() << "Native calendar benchmark prepared";
    QTimer::singleShot(1000, this, [this] { beginSample(); });
}
bool GuiCalendarBenchmark::click(const QString &name) {
    auto item = findItem(m_window->contentItem(), name);
    const auto rect = visibleRect(item, m_window);
    if (!item || !item->isEnabled() || rect.width() < 4 || rect.height() < 4) return false;
    const auto local = rect.center(), global = QPointF(m_window->mapToGlobal(local.toPoint()));
    QMouseEvent move(QEvent::MouseMove, local, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    QMouseEvent press(QEvent::MouseButtonPress, local, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, local, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &move);
    QCoreApplication::sendEvent(m_window, &press);
    QCoreApplication::sendEvent(m_window, &release);
    m_input["scene_position"] = QJsonObject{{"x", local.x()}, {"y", local.y()}};
    return press.isAccepted() || release.isAccepted();
}
void GuiCalendarBenchmark::beginSample() {
    if (m_phase == Phase::Finished) return;
    if (!m_window->isActive() || !m_window->isExposed()) { finish("The native benchmark window lost activation or exposure."); return; }
    const auto view = m_backend.calendarView();
    const auto date = QDate::fromString(m_backend.calendarDate(), Qt::ISODate);
    QString control;
    m_setup = false; m_targetDate = m_backend.calendarDate();
    // Routes needing a particular starting view first switch to it unmeasured.
    const QString needed = m_class == 1 || m_class == 3 ? "month" : m_class == 2 ? "week" : "";
    if (!needed.isEmpty() && view != needed) {
        m_setup = true; m_targetView = needed; control = "calendarView_" + needed;
    } else if (m_class == 0) {
        m_targetView = view == "week" ? "month" : view == "month" ? "day" : "week";
        control = "calendarView_" + m_targetView;
    } else if (m_class == 1) {
        // Days from the second to the fifth grid row, spread across the month.
        const int index = 7 + (m_iteration * 5) % 28;
        m_targetView = "day";
        m_targetDate = QDate::fromString(m_backend.calendarFrom(), Qt::ISODate).addDays(index).toString(Qt::ISODate);
        control = "calendarMonthDate-" + QString::number(index);
    } else {
        // Eight weeks or four months forward, then back, so dates stay near the data.
        const int direction = (m_iteration / (m_class == 2 ? 8 : 4)) % 2 == 0 ? 1 : -1;
        m_targetView = view;
        m_targetDate = (m_class == 2 ? date.addDays(7 * direction) : date.addMonths(direction)).toString(Qt::ISODate);
        control = direction > 0 ? "calendarNext" : "calendarPrevious";
    }
    m_input = {{"input_object", control}, {"old_view", view}, {"old_date", date.toString(Qt::ISODate)},
        {"target_view", m_targetView}, {"target_date", m_targetDate}};
    m_modelAt = 0; m_intermediateFrames = 0; m_modelInvalidations = 0;
    m_armedGeneration = 0; m_armedSnapshot = {}; m_paint->arm(0);
    m_phase = Phase::AwaitingModel; m_watchdog.start(10000); m_inspectTimer.start();
    m_inputAt = GuiPaintObserver::nowNanos();
    if (!click(control)) { finish("The Calendar control could not receive native mouse input: " + control); return; }
    inspect();
}
bool GuiCalendarBenchmark::modelCorrect() const {
    if (m_backend.calendarView() != m_targetView || m_backend.calendarDate() != m_targetDate || !m_backend.calendarReady()) return false;
    // Events must be those published for exactly the displayed range.
    const auto exact = m_backend.calendarWindow(m_backend.calendarFrom(), m_backend.calendarDays());
    return m_backend.m_calendarDisplayKey.endsWith('#' + exact.key);
}
bool GuiCalendarBenchmark::visibleSnapshot(QJsonObject &snapshot) const {
    if (!modelCorrect() || !m_window->isExposed()) return false;
    auto calendar = findItem(m_window->contentItem(), "calendarWorkspace");
    auto title = calendar ? findItem(calendar, "calendarTitle") : nullptr;
    auto dateButton = calendar ? findItem(calendar, "calendarDatePickerButton") : nullptr;
    if (!title || !dateButton) return false;
    // The rows this view shows for the saved account visibility.
    const bool combined = calendar->property("combinedAccounts").toBool();
    QVariantList rows; QSet<QString> rowNames;
    for (const auto &value : m_backend.events()) {
        const auto row = value.toMap();
        if (!combined && row.value("account") != m_backend.account()) continue;
        rows.append(row); rowNames.insert(eventName(row));
    }
    const auto from = QDate::fromString(m_backend.calendarFrom(), Qt::ISODate);
    QJsonArray painted, identities;
    if (m_targetView == "day" || m_targetView == "week") {
        const int days = m_targetView == "day" ? 1 : 7;
        QStringList expected;
        for (const auto &value : rows) {
            const auto row = value.toMap();
            const int day = row.value("day_index", -1).toInt();
            if (row.value("all_day").toBool() || day < 0 || day >= days) continue;
            expected.append("calendarEvent-" + eventName(row));
            identities.append(QJsonObject{{"account", row.value("account").toString()}, {"calendar_id", row.value("calendar_id").toString()},
                {"event_id", row.value("event_id").toString()}, {"start", row.value("start").toString()}});
        }
        auto actual = names(ordered(calendar, "calendarEvent-"));
        std::sort(expected.begin(), expected.end()); std::sort(actual.begin(), actual.end());
        expected.removeDuplicates(); actual.removeDuplicates();
        if (expected != actual) return false;
        painted = QJsonArray::fromStringList(actual);
        snapshot["all_day"] = QJsonArray::fromStringList(names(ordered(calendar, "calendarAllDay-")));
    } else if (m_targetView == "month") {
        const auto cells = ordered(calendar, "calendarMonthCell-");
        if (cells.size() != calendar->property("monthCells").toInt()) return false;
        QHash<QString, QStringList> shownByDay;
        for (int index = 0; index < cells.size(); ++index) {
            const auto iso = from.addDays(index).toString(Qt::ISODate);
            auto date = findItem(cells[index], "calendarMonthDate-" + QString::number(index));
            if (cells[index]->objectName() != "calendarMonthCell-" + iso || !date
                || date->property("text").toString() != QString::number(from.addDays(index).day())) return false;
            // Each cell shows the first rows of its own day that fit.
            const auto dayEvents = cells[index]->property("dayEvents").toList();
            const int shown = std::min<int>(cells[index]->property("shown").toInt(), std::min<qsizetype>(3, dayEvents.size()));
            QStringList expected;
            for (int row = 0; row < dayEvents.size(); ++row) {
                const auto name = eventName(dayEvents[row].toMap());
                if (!rowNames.contains(name)) return false;
                if (row < shown) expected.append("calendarMonthEvent-" + name);
            }
            const auto actual = names(ordered(cells[index], "calendarMonthEvent-"));
            if (expected != actual) return false;
            if (!actual.isEmpty()) { shownByDay[iso] = actual; painted.append(QJsonObject{{"date", iso}, {"events", QJsonArray::fromStringList(actual)}}); }
        }
        // Every published row on a visible day reaches that day's cell.
        for (const auto &value : rows) {
            const auto row = value.toMap();
            const auto date = row.value("date").toString();
            if (date.isEmpty() || QDate::fromString(date, Qt::ISODate) < from || QDate::fromString(date, Qt::ISODate) >= from.addDays(cells.size())) continue;
            const auto cell = cells[from.daysTo(QDate::fromString(date, Qt::ISODate))];
            bool present = false;
            for (const auto &entry : cell->property("dayEvents").toList()) present |= eventName(entry.toMap()) == eventName(row);
            if (!present) return false;
            identities.append(QJsonObject{{"account", row.value("account").toString()}, {"calendar_id", row.value("calendar_id").toString()},
                {"event_id", row.value("event_id").toString()}, {"start", row.value("start").toString()}});
        }
        snapshot["cells"] = cells.size();
    } else return false;
    snapshot["view"] = m_targetView; snapshot["date"] = m_targetDate; snapshot["from"] = m_backend.calendarFrom();
    snapshot["title"] = title->property("text").toString(); snapshot["date_button"] = dateButton->property("text").toString();
    snapshot["painted"] = painted; snapshot["identities"] = identities; snapshot["published_rows"] = rows.size();
    return true;
}
void GuiCalendarBenchmark::inspect() {
    if (m_phase != Phase::AwaitingModel && m_phase != Phase::AwaitingPaint) return;
    if (!modelCorrect()) {
        if (m_phase == Phase::AwaitingPaint) ++m_modelInvalidations;
        m_paint->arm(0); m_armedGeneration = 0; m_phase = Phase::AwaitingModel; return;
    }
    if (m_modelAt == 0) m_modelAt = GuiPaintObserver::nowNanos();
    QJsonObject snapshot;
    if (!visibleSnapshot(snapshot)) return;
    if (m_phase == Phase::AwaitingPaint && snapshot == m_armedSnapshot) return;
    m_armedSnapshot = snapshot; m_armedGeneration = ++m_generation; m_phase = Phase::AwaitingPaint;
    m_paint->arm(m_armedGeneration);
    m_window->update();
}
void GuiCalendarBenchmark::painted(GuiPaintObserver::Frame frame) {
    if (m_phase == Phase::AwaitingModel) { ++m_intermediateFrames; return; }
    if (m_phase != Phase::AwaitingPaint) return;
    if (!m_armedGeneration || frame.generation != m_armedGeneration || frame.serial <= m_lastSerial) { ++m_intermediateFrames; return; }
    QJsonObject snapshot;
    if (!visibleSnapshot(snapshot) || snapshot != m_armedSnapshot) { ++m_intermediateFrames; inspect(); return; }
    if (frame.swappedAt < m_modelAt || m_modelAt < m_inputAt) { finish("A pre-input or pre-model frame reached calendar measurement."); return; }
    m_lastSerial = frame.serial; m_watchdog.stop(); m_inspectTimer.stop(); m_paint->arm(0); m_armedGeneration = 0;
    m_phase = Phase::BetweenSamples;
    if (!m_setup) {
        const double total = double(frame.swappedAt - m_inputAt) / 1000000.0;
        for (auto it = m_input.begin(); it != m_input.end(); ++it) snapshot[it.key()] = it.value();
        snapshot["input_to_paint_ms"] = total; snapshot["input_to_model_ms"] = double(m_modelAt - m_inputAt) / 1000000.0;
        snapshot["model_to_paint_ms"] = double(frame.swappedAt - m_modelAt) / 1000000.0;
        snapshot["frames_before_correct"] = m_intermediateFrames; snapshot["model_invalidations"] = m_modelInvalidations;
        snapshot["calendar_reads_pending_at_paint"] = int(m_backend.m_calendarWindowRequests.size());
        if (m_iteration > 0) { snapshot["sample"] = m_iteration; m_samples[m_class].append(snapshot); }
        if (++m_iteration > 100) {
            qInfo() << "Native calendar class completed:" << labels[m_class] << "100 measured correct grid frames";
            m_iteration = 0;
            if (++m_class == 4) { finish(); return; }
        }
    }
    QTimer::singleShot(25, this, [this] { beginSample(); });
}
void GuiCalendarBenchmark::finish(const QString &error) {
    if (m_phase == Phase::Finished) return;
    const int interruptedClass = m_class, interruptedSample = m_iteration;
    stopForShutdown();
    QJsonArray classes; bool passed = error.isEmpty();
    for (int index = 0; index < 4; ++index) {
        auto row = summary(m_samples[index], "input_to_paint_ms");
        row["class"] = labels[index]; row["observations"] = m_samples[index].size(); row["samples"] = m_samples[index];
        row["input_to_model"] = summary(m_samples[index], "input_to_model_ms");
        row["model_to_paint"] = summary(m_samples[index], "model_to_paint_ms");
        const bool classPassed = m_samples[index].size() == 100 && row["p95_ms"].toDouble(1000) <= 100;
        row["budget_passed"] = classPassed; passed &= classPassed; classes.append(row);
    }
    m_budgetPassed = passed;
    const QJsonObject report{{"result", error.isEmpty() ? "measured" : "failed"}, {"error", error}, {"classes", classes},
        {"budget_passed", passed}, {"p95_budget_ms", 100},
        {"interrupted_class", interruptedClass}, {"interrupted_sample", interruptedSample}, {"interrupted_input", m_input},
        {"boundary", "Native QQuickWindow mouse input on the actual Calendar control to the first frame whose view, date, exact-range published events and visible event delegates match. Generation captured afterSynchronizing, carried through afterRendering and directly timestamped frameSwapped."},
        {"oracle", "Day/week: visible timed-event delegates equal the published timed rows for the displayed days. Month: every visible cell carries its own date, shows the first rows of its day that fit, and every published row on a visible day reaches that day's cell. Painted identities are recorded for independent store verification."},
        {"sample_policy", "One unmeasured warmup per route; 100 measured inputs per route; next input 25 ms after the prior correct paint. Routes: Day/Week/Month segment cycle; a month-grid date click into Day; Week next/previous eight weeks each way; Month next/previous four months each way. Starting-view switches are unmeasured. Normal polling, idle neighbour prefetch and serial backend queues remain enabled."},
        {"host", QSysInfo::prettyProductName()}, {"architecture", QSysInfo::currentCpuArchitecture()},
        {"qt_version", QT_VERSION_STR}, {"renderer_api", int(m_window->rendererInterface()->graphicsApi())},
        {"window_width", m_window->width()}, {"window_height", m_window->height()}, {"device_pixel_ratio", m_window->devicePixelRatio()},
        {"synchronizations", qint64(m_paint->syncCount())}, {"renders", qint64(m_paint->renderCount())}, {"swaps", qint64(m_paint->swapCount())}};
    QSaveFile output(m_output);
    const auto bytes = QJsonDocument(report).toJson();
    if (!output.open(QIODevice::WriteOnly) || output.write(bytes) != bytes.size() || !output.commit()) {
        m_budgetPassed = false; qWarning() << "Could not write calendar benchmark report:" << m_output;
    }
    if (!error.isEmpty()) qWarning() << "Native calendar benchmark failed:" << error;
    m_backend.shutdown();
}
