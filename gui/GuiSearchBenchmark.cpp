#include "GuiSearchBenchmark.h"
#include <QCoreApplication>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QJsonDocument>
#include <QSaveFile>
#include <QSysInfo>
#include <QSGRendererInterface>
#include <QRegularExpression>
#include <QtMath>
#include <algorithm>
#include <chrono>

namespace {
qint64 monotonicNanos() {
    return std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now().time_since_epoch()).count();
}
const QStringList queries{"needlequarter", "ultraquartz", "", "deployment"};
QStringList generatedRareIds(int queryClass) {
    QList<QPair<QString, QString>> matches;
    for (int index = 0; index < 100000; index += 2) {
        if (queryClass == 0 ? index % 10000 != 0 : index % 997 != 0) continue;
        const auto date = QString("2026-09-%1T%2:%3:%4Z").arg(1 + index % 11, 2, 10, QLatin1Char('0'))
            .arg(index % 24, 2, 10, QLatin1Char('0')).arg(index % 60, 2, 10, QLatin1Char('0')).arg((index / 60) % 60, 2, 10, QLatin1Char('0'));
        matches.append({date, QString("benchmark-windscribe-thread-%1").arg(index, 6, 10, QLatin1Char('0'))});
    }
    std::sort(matches.begin(), matches.end(), [](const auto &a, const auto &b) { return a.first != b.first ? a.first > b.first : a.second < b.second; });
    QStringList ids;
    for (const auto &match : matches) { if (ids.size() == 50) break; ids.append(match.second); }
    return ids;
}
}

GuiSearchBenchmark::GuiSearchBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_output(std::move(output)) {
    m_startTimer.setInterval(100);
    connect(&m_startTimer, &QTimer::timeout, this, [this] {
        if (!m_backend.connected() || m_backend.loading() || m_backend.conversation().isEmpty() || m_backend.draft().isEmpty()) return;
        if (m_backend.account() != "windscribe") { m_backend.selectAccount("windscribe"); return; }
        if (m_backend.currentAccount()["message_count"].toInt() < 50000) { finish("Use the retained generated 100k benchmark profile."); return; }
        m_search = m_window->findChild<QQuickItem *>("searchField");
        if (!m_search) { finish("The actual QML search field was not found."); return; }
        m_startTimer.stop(); m_backend.setFolder("all"); m_backend.setCategory({});
        QTimer::singleShot(500, this, [this] { beginSample(); });
    });
    connect(&m_backend, &MailBackend::loadingChanged, this, [this] {
        if (m_finished || (m_phase != Phase::ResetQuery && m_phase != Phase::TargetQuery) || m_backend.query() != m_expected) return;
        if (m_backend.loading()) m_loadingSeen = true;
        else if (m_loadingSeen) resultsReady();
    });
    connect(&m_backend, &MailBackend::conversationsChanged, this, [this] {
        if ((m_phase == Phase::ResetQuery || m_phase == Phase::TargetQuery) && m_loadingSeen && m_backend.query() == m_expected) m_modelChanged = true;
    });
    // A queued frameSwapped signal can arrive after arming even though its
    // frame was rendered before the replacement model. Capture generation at
    // synchronization, carry it through rendering, then capture it directly at
    // swap time. The GUI callback accepts only that exact generation.
    connect(m_window, &QQuickWindow::afterSynchronizing, this, [this] {
        ++m_syncSerial; m_syncGeneration = m_requestedGeneration.load(std::memory_order_acquire);
    }, Qt::DirectConnection);
    connect(m_window, &QQuickWindow::afterRendering, this, [this] {
        m_renderedSerial = m_syncSerial; m_renderedGeneration = m_syncGeneration;
    }, Qt::DirectConnection);
    connect(m_window, &QQuickWindow::frameSwapped, this, [this] {
        const auto generation = m_renderedGeneration, serial = m_renderedSerial;
        const auto swappedAt = monotonicNanos();
        QMetaObject::invokeMethod(this, [this, generation, serial, swappedAt] { painted(generation, serial, swappedAt); }, Qt::QueuedConnection);
    }, Qt::DirectConnection);
    connect(&m_backend, &MailBackend::errorChanged, this, [this] { if (!m_backend.error().isEmpty()) finish(m_backend.error()); });
    QTimer::singleShot(600000, this, [this] { if (!m_finished) finish("The search benchmark timed out."); });
    m_startTimer.start();
}
void GuiSearchBenchmark::beginSample() {
    if (m_finished) return;
    m_phase = Phase::Preparing;
    const QVariantMap filters = m_class == 3 ? QVariantMap{
        {"sender", "alex.o'hara+build@windscribe.example"}, {"status", "review"},
        {"has_attachment", false}, {"after", "2026-09-01"}, {"before", "2026-09-12"}} : QVariantMap{};
    if (m_backend.filters() != filters) {
        m_backend.applyFilters(filters);
        QTimer::singleShot(400, this, [this] { beginSample(); }); return;
    }
    if (m_backend.loading()) { QTimer::singleShot(50, this, [this] { beginSample(); }); return; }
    // Every measured target replaces a painted zero-result model. Whitespace
    // changes are insufficient: they can produce identical rows and no QML
    // model replacement after warmup.
    inputQuery(QString("fmbenchmarknomatch%1x%2").arg(m_class).arg(m_iteration), Phase::ResetQuery);
}
void GuiSearchBenchmark::stopForShutdown() {
    m_finished = true; m_startTimer.stop();
    m_requestedGeneration.store(0, std::memory_order_release);
}
void GuiSearchBenchmark::inputQuery(const QString &query, Phase phase) {
    m_expected = query; m_phase = phase;
    m_loadingSeen = false; m_modelChanged = false; m_armedGeneration = 0;
    m_requestedGeneration.store(0, std::memory_order_release);
    m_search->forceActiveFocus();
    QMetaObject::invokeMethod(m_search, "selectAll");
    m_inputAt = monotonicNanos();
    if (m_expected.isEmpty()) {
        QKeyEvent key(QEvent::KeyPress, Qt::Key_Backspace, Qt::NoModifier);
        QCoreApplication::sendEvent(m_search, &key);
    } else {
        QInputMethodEvent input; input.setCommitString(m_expected);
        QCoreApplication::sendEvent(m_search, &input);
    }
    if (m_backend.query() != m_expected) finish("QML input did not update the shared query.");
}
void GuiSearchBenchmark::resultsReady() {
    if (!m_modelChanged) { finish("Fresh query completed without a conversation model replacement."); return; }
    const auto rows = m_backend.conversations();
    if (m_phase == Phase::ResetQuery) {
        if (!rows.isEmpty() || m_backend.querySummary()["filtered_conversation_count"].toInt() != 0) { finish("The unmeasured reset query did not produce an empty model."); return; }
        m_phase = Phase::ResetPaint;
    } else {
        const auto error = validateTarget(rows);
        if (!error.isEmpty()) { finish(error); return; }
        m_phase = Phase::TargetPaint;
    }
    m_armedGeneration = ++m_generation;
    m_requestedGeneration.store(m_armedGeneration, std::memory_order_release);
    m_window->update();
}
QString GuiSearchBenchmark::validateTarget(const QVariantList &rows) {
    const int expectedCount = m_class == 0 ? 10 : 50;
    if (rows.size() != expectedCount) return "The fresh first result page has an unexpected row count.";
    QStringList ids;
    static const QRegularExpression generated("^benchmark-windscribe-thread-[0-9]{6}$"), ingested("^ingest-[0-9]{3}-[0-9]{2}$");
    for (const auto &row : rows) {
        const auto value = row.toMap(); const auto id = value["id"].toString();
        if ((!generated.match(id).hasMatch() && !(m_class >= 2 && ingested.match(id).hasMatch()))
            || value["message_count"].toInt() != 1 || value["sender_email"].toString() != "alex.o'hara+build@windscribe.example"
            || value["group"].toString() != "review") return "Search returned an unexpected fixture identity, account, or group.";
        if (ids.contains(id)) return "The fresh first page contains a duplicate conversation.";
        ids.append(id);
    }
    if (m_expectedIds[m_class].isEmpty()) m_expectedIds[m_class] = m_class < 2 ? generatedRareIds(m_class) : ids;
    if (ids != m_expectedIds[m_class]) return "The fresh first-page IDs differ from the expected sequence.";
    return {};
}
void GuiSearchBenchmark::painted(quint64 generation, quint64 serial, qint64 swappedAt) {
    if (m_finished || !m_armedGeneration || generation != m_armedGeneration || serial <= m_lastPaintSerial) return;
    if (m_backend.loading() || m_backend.query() != m_expected) { finish("The query changed before its validated replacement frame was painted."); return; }
    m_lastPaintSerial = serial; m_armedGeneration = 0;
    m_requestedGeneration.store(0, std::memory_order_release);
    if (m_phase == Phase::ResetPaint) {
        if (!m_backend.conversations().isEmpty()) { finish("Reset model changed before its paint was accepted."); return; }
        if (m_iteration > 0) ++m_resetPaints[m_class];
        inputQuery(queries[m_class], Phase::TargetQuery); return;
    }
    if (m_phase != Phase::TargetPaint) { finish("A frame was accepted outside the expected target phase."); return; }
    const auto error = validateTarget(m_backend.conversations());
    if (!error.isEmpty()) { finish(error); return; }
    const double milliseconds = (swappedAt - m_inputAt) / 1000000.0;
    if (milliseconds < 0) { finish("A pre-input frame was supplied to the measurement."); return; }
    if (m_iteration > 0) m_samples[m_class].append(milliseconds); // one warm-up per class
    if (m_iteration > 0) ++m_modelReplacements[m_class];
    if (++m_iteration > 100) { m_iteration = 0; ++m_class; }
    if (m_class == 4) { finish(); return; }
    QTimer::singleShot(25, this, [this] { beginSample(); });
}
void GuiSearchBenchmark::finish(const QString &error) {
    if (m_finished) return;
    m_finished = true; m_startTimer.stop();
    static const QStringList labels{"rare subject", "rare body", "grouped first50", "combined filters first50"};
    QJsonArray classes; bool budgetPassed = error.isEmpty();
    for (int index = 0; index < 4; ++index) {
        auto values = m_samples[index]; std::sort(values.begin(), values.end());
        QJsonObject row{{"class", labels[index]}, {"observations", values.size()},
            {"target_model_replacements", m_modelReplacements[index]}, {"unmeasured_empty_reset_paints", m_resetPaints[index]},
            {"expected_first_page_ids", QJsonArray::fromStringList(m_expectedIds[index])},
            {"identity_oracle", index < 2 ? "Deterministic generated rare-query IDs sorted by fixture date and ID" : "Fixture-domain-validated warmup first page; exact ordered IDs required on every measured replacement"}};
        if (!values.isEmpty()) {
            auto percentile = [&values](double q) { return values[qCeil((values.size() - 1) * q)]; };
            row["p50_ms"] = percentile(.5); row["p95_ms"] = percentile(.95); row["p99_ms"] = percentile(.99);
            row["max_ms"] = values.last(); budgetPassed &= values.size() == 100 && m_modelReplacements[index] == 100 && m_resetPaints[index] == 100 && percentile(.95) <= 200;
        } else budgetPassed = false;
        classes.append(row);
    }
    const QJsonObject report{{"result", error.isEmpty() ? "measured" : "failed"}, {"error", error},
        {"boundary", "Final QML target input event to directly timestamped frameSwapped; frame generation captured afterSynchronizing and afterRendering must match the armed replacement model. Includes debounce, engine queue, IPC and rendering."},
        {"host", QSysInfo::prettyProductName()}, {"architecture", QSysInfo::currentCpuArchitecture()},
        {"qt_version", QT_VERSION_STR}, {"renderer_api", int(m_window->rendererInterface()->graphicsApi())},
        {"p95_budget_ms", 200}, {"budget_passed", budgetPassed}, {"queries", classes},
        {"reset_policy", "Before every target: actual unique no-match QML input, confirmed zero-row model replacement, and generation-matched empty frame; all reset work is outside the sample. One unmeasured target warmup per class."},
        {"limits", "Retained synthetic 100k index-density fixture, including retained ingest-* contention rows when present; warm OS cache; no concurrent source ingestion or model inference during this GUI run; Core contention measured separately."}};
    QSaveFile output(m_output);
    if (!output.open(QIODevice::WriteOnly) || output.write(QJsonDocument(report).toJson()) < 0 || !output.commit()) { QCoreApplication::exit(8); return; }
    if (!error.isEmpty()) { QCoreApplication::exit(7); return; }
    m_backend.shutdown();
}
