#include "GuiInteractionBenchmark.h"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDateTime>
#include <QDebug>
#include <QFileInfo>
#include <QImage>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QSaveFile>
#include <QSGRendererInterface>
#include <QSysInfo>
#include <QtMath>
#include <algorithm>

namespace {
const QStringList labels{"mailbox_click", "mailbox_tab", "mailbox_shift_tab", "conversation_click", "large_mailbox_conversation_click"};
QQuickItem *findItem(QQuickItem *parent, const QString &name) {
    if (!parent->isVisible()) return nullptr;
    if (parent->objectName() == name) return parent;
    for (auto child : parent->childItems()) if (auto found = findItem(child, name)) return found;
    return nullptr;
}
QQuickItem *findAnyItem(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name) return parent;
    for (auto child : parent->childItems()) if (auto found = findAnyItem(child, name)) return found;
    return nullptr;
}
QRectF visibleRect(QQuickItem *item, QQuickWindow *window) {
    if (!item || !item->isVisible() || item->opacity() <= 0 || item->width() <= 0 || item->height() <= 0) return {};
    auto rect = item->mapRectToScene(QRectF(0, 0, item->width(), item->height()));
    rect = rect.intersected(QRectF(0, 0, window->width(), window->height()));
    for (auto parent = item->parentItem(); parent; parent = parent->parentItem()) {
        if (!parent->isVisible() || parent->opacity() <= 0) return {};
        if (parent->clip()) rect = rect.intersected(parent->mapRectToScene(QRectF(0, 0, parent->width(), parent->height())));
    }
    return rect;
}
void namedItems(QQuickItem *parent, const QString &prefix, QList<QQuickItem *> &result) {
    if (!parent->isVisible()) return;
    if (parent->objectName().startsWith(prefix)) result.append(parent);
    for (auto child : parent->childItems()) namedItems(child, prefix, result);
}
QList<QQuickItem *> visibleItems(QQuickWindow *window, const QString &prefix) {
    QList<QQuickItem *> items, visible;
    namedItems(window->contentItem(), prefix, items);
    for (auto item : items) {
        const auto rect = visibleRect(item, window);
        if (rect.width() > 16 && rect.height() > 8) visible.append(item);
    }
    std::sort(visible.begin(), visible.end(), [](auto a, auto b) {
        const auto ap = a->mapToScene(QPointF{}), bp = b->mapToScene(QPointF{});
        return ap.y() != bp.y() ? ap.y() < bp.y() : ap.x() < bp.x();
    });
    return visible;
}
QQuickItem *bodyItem(QQuickItem *parent, const QString &text, QQuickWindow *window) {
    if (!parent->isVisible()) return nullptr;
    if (parent->property("readOnly").toBool() && parent->property("text").toString() == text
        && visibleRect(parent, window).width() > 16 && visibleRect(parent, window).height() > 8) return parent;
    for (auto child : parent->childItems()) if (auto found = bodyItem(child, text, window)) return found;
    return nullptr;
}
QJsonArray rowIds(const QVariantList &rows) {
    QJsonArray ids;
    for (const auto &row : rows) ids.append(row.toMap()["id"].toString());
    return ids;
}
QJsonObject summary(const QJsonArray &samples, const QString &field) {
    QList<double> values;
    for (const auto &sample : samples) values.append(sample.toObject()[field].toDouble());
    if (values.isEmpty()) return {};
    std::sort(values.begin(), values.end());
    auto percentile = [&values](double p) { return values[qCeil((values.size() - 1) * p)]; };
    return {{"p50_ms", percentile(.5)}, {"p95_ms", percentile(.95)}, {"p99_ms", percentile(.99)}, {"max_ms", values.last()}};
}
QJsonObject rectJson(const QRectF &rect) {
    return {{"x", rect.x()}, {"y", rect.y()}, {"width", rect.width()}, {"height", rect.height()}};
}
QJsonObject itemDiagnostic(QQuickItem *item, QQuickWindow *window) {
    if (!item) return {{"found", false}};
    const auto text = item->property("text").toString();
    QJsonObject result{{"found", true}, {"name", item->objectName()}, {"type", item->metaObject()->className()},
        {"visible", item->isVisible()}, {"enabled", item->isEnabled()}, {"opacity", item->opacity()}, {"clip", item->clip()},
        {"scene_rect", rectJson(item->mapRectToScene(QRectF(0, 0, item->width(), item->height())))},
        {"visible_rect", rectJson(visibleRect(item, window))}, {"implicit_width", item->implicitWidth()}, {"implicit_height", item->implicitHeight()}};
    for (const auto key : {"contentY", "contentHeight", "originY", "count", "readOnly", "isExpanded"}) {
        const auto value = item->property(key);
        if (value.isValid()) result[key] = QJsonValue::fromVariant(value);
    }
    if (item->property("text").isValid()) {
        result["text_length"] = text.size();
        result["text_sha256"] = QString::fromLatin1(QCryptographicHash::hash(text.toUtf8(), QCryptographicHash::Sha256).toHex());
    }
    return result;
}
void collectDiagnostics(QQuickItem *parent, QQuickWindow *window, QJsonArray &rows, QJsonArray &members, QJsonArray &bodies) {
    if (parent->objectName().startsWith("conversationRow_")) rows.append(itemDiagnostic(parent, window));
    if (parent->objectName().startsWith("messageMember_")) members.append(itemDiagnostic(parent, window));
    if (parent->property("readOnly").toBool()) bodies.append(itemDiagnostic(parent, window));
    for (auto child : parent->childItems()) collectDiagnostics(child, window, rows, members, bodies);
}
}

GuiInteractionBenchmark::GuiInteractionBenchmark(MailBackend &backend, QQuickWindow *window, QString output, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window),
      m_paint(new GuiPaintObserver(window, [this](auto frame) { painted(frame); }, this)), m_output(std::move(output)) {
    m_window->raise(); m_window->requestActivate();
    m_startTimer.setInterval(50);
    connect(&m_startTimer, &QTimer::timeout, this, &GuiInteractionBenchmark::startWhenReady);
    m_inspectTimer.setInterval(5); m_inspectTimer.setTimerType(Qt::PreciseTimer);
    connect(&m_inspectTimer, &QTimer::timeout, this, &GuiInteractionBenchmark::inspect);
    m_watchdog.setSingleShot(true);
    connect(&m_watchdog, &QTimer::timeout, this, [this] {
        finish(QString("Navigation stalled: phase=%1 class=%2 sample=%3 account=%4 selected=%5 targetAccount=%6 targetSelected=%7 loading=%8 active=%9 exposed=%10")
            .arg(int(m_phase)).arg(m_class).arg(m_iteration).arg(m_backend.account(), m_backend.selectedId(), m_targetAccount, m_targetSelected)
            .arg(m_backend.loading()).arg(m_window->isActive()).arg(m_window->isExposed()));
    });
    for (auto signal : {&MailBackend::conversationChanged, &MailBackend::conversationsChanged, &MailBackend::accountChanged, &MailBackend::loadingChanged})
        connect(&m_backend, signal, this, &GuiInteractionBenchmark::inspect);
    connect(&m_backend, &MailBackend::errorChanged, this, [this] {
        if (!m_backend.error().isEmpty()) finish(m_backend.error());
    });
    QTimer::singleShot(600000, this, [this] { if (m_phase != Phase::Finished) finish("The navigation benchmark exceeded its total watchdog."); });
    m_startTimer.start(); m_watchdog.start(45000);
}
void GuiInteractionBenchmark::stopForShutdown() {
    m_phase = Phase::Finished; m_startTimer.stop(); m_inspectTimer.stop(); m_watchdog.stop(); m_paint->arm(0);
}
void GuiInteractionBenchmark::startWhenReady() {
    if (m_phase != Phase::Starting || !m_window->isActive() || !m_window->isExposed() || !m_backend.connected()
        || m_backend.loading() || m_backend.conversations().isEmpty() || m_backend.conversation().isEmpty() || m_backend.draft().isEmpty()) return;
    if (m_window->property("workspace").toString() != "mail" || m_window->property("settingsOpen").toBool()
        || m_backend.folder() != "inbox" || !m_backend.query().isEmpty() || !m_backend.category().isEmpty()
        || !m_backend.filters().isEmpty() || !m_backend.activeDraftId().isEmpty()) {
        finish("Prepare the disposable profile in Mail/Inbox with empty filters and no active independent draft."); return;
    }
    for (const auto &account : m_backend.accounts()) {
        const auto values = account.toMap();
        m_accounts.append(values["id"].toString());
        if (values["conversation_count"].toLongLong() > m_largeMailboxCount) {
            m_largeMailbox = values["id"].toString(); m_largeMailboxCount = values["conversation_count"].toLongLong();
        }
    }
    if (m_largeMailboxCount >= 10000) m_routeCount = 5;
    if (m_accounts.size() < 2) { finish("Navigation measurement needs at least two separate accounts."); return; }
    m_targetAccount = m_backend.account(); m_targetSelected = m_backend.selectedId();
    QJsonObject snapshot;
    if (!visibleSnapshot(snapshot)) { m_accounts.clear(); return; }
    m_restoredSelections[m_backend.account()] = m_backend.selectedId();
    m_startTimer.stop(); m_watchdog.stop(); m_phase = Phase::BetweenSamples;
    qInfo() << "Native interaction benchmark prepared";
    QTimer::singleShot(500, this, [this] { beginSample(); });
}
bool GuiInteractionBenchmark::click(QQuickItem *item) {
    if (!item || !item->isEnabled()) return false;
    const auto rect = visibleRect(item, m_window);
    if (rect.width() < 16 || rect.height() < 8) return false;
    const auto local = rect.center(), global = QPointF(m_window->mapToGlobal(local.toPoint()));
    QMouseEvent move(QEvent::MouseMove, local, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    QMouseEvent press(QEvent::MouseButtonPress, local, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, local, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &move);
    QCoreApplication::sendEvent(m_window, &press); QCoreApplication::sendEvent(m_window, &release);
    return true;
}
void GuiInteractionBenchmark::prepareLargeMailbox() {
    // The original four routes remain byte-for-byte the same workload. This
    // extra setup click and its verified paint are outside the fifth route.
    m_preparingLargeMailbox = true;
    m_input = {{"setup", "large_mailbox"}, {"old_account", m_backend.account()}, {"old_selected_id", m_backend.selectedId()},
        {"input_object", "accountTab_" + m_largeMailbox}};
    m_targetAccount = m_largeMailbox; m_targetSelected = m_restoredSelections.value(m_largeMailbox); m_targetMessage.clear();
    m_modelAt = 0; m_visibleAt = 0; m_modelInvalidations = 0; m_visibilityWaits = 0;
    m_armedGeneration = 0; m_armedSnapshot = {}; m_paint->arm(0);
    m_phase = Phase::AwaitingModel; m_watchdog.start(10000); m_inspectTimer.start();
    m_inputUnixMs = QDateTime::currentMSecsSinceEpoch(); m_inputAt = GuiPaintObserver::nowNanos();
    if (!click(findItem(m_window->contentItem(), "accountTab_" + m_largeMailbox))) { finish("Could not click the largest mailbox for the added conversation route."); return; }
    inspect();
}
void GuiInteractionBenchmark::beginSample() {
    if (m_phase == Phase::Finished) return;
    if (!m_window->isActive() || !m_window->isExposed()) { finish("The native benchmark window lost activation or exposure."); return; }
    if (m_backend.draftConflict() || m_backend.draftLocked()) { finish("Draft recovery requires user action; the benchmark will not change authored work."); return; }
    m_restoredSelections[m_backend.account()] = m_backend.selectedId();
    m_input = {{"old_account", m_backend.account()}, {"old_selected_id", m_backend.selectedId()}};
    m_targetAccount = m_backend.account(); m_targetSelected.clear(); m_targetMessage.clear();
    QQuickItem *control = nullptr;
    if (m_class < 3) {
        const auto index = m_accounts.indexOf(m_backend.account());
        if (index < 0) { finish("The active mailbox is outside the startup account inventory."); return; }
        const int direction = m_class == 2 ? -1 : 1;
        m_targetAccount = m_accounts[(index + direction + m_accounts.size()) % m_accounts.size()];
        m_targetSelected = m_restoredSelections.value(m_targetAccount);
        m_input["wrapped"] = direction > 0 ? index == m_accounts.size() - 1 : index == 0;
        if (m_class == 0) {
            control = findItem(m_window->contentItem(), "accountTab_" + m_targetAccount);
            m_input["input_object"] = "accountTab_" + m_targetAccount;
        } else {
            auto mail = findItem(m_window->contentItem(), "mailWorkspace");
            if (!mail) { finish("The neutral Mail workspace focus target is unavailable."); return; }
            mail->forceActiveFocus(Qt::OtherFocusReason);
            if (!m_window->property("accountKeysAllowed").toBool()) { finish("Mail account shortcuts are unavailable from neutral workspace focus."); return; }
            m_input["input_object"] = m_class == 1 ? "QQuickWindow:Tab" : "QQuickWindow:Shift+Tab";
        }
    } else {
        const auto visible = visibleItems(m_window, "conversationRow_");
        if (visible.size() < 2) { finish("At least two actual conversation rows must be visible for the click route."); return; }
        int selected = -1;
        for (int i = 0; i < visible.size(); ++i) if (visible[i]->objectName() == "conversationRow_" + m_backend.selectedId()) selected = i;
        control = visible[(selected + 1) % visible.size()];
        m_targetSelected = control->objectName().mid(QStringLiteral("conversationRow_").size());
        m_input["input_object"] = control->objectName();
        for (const auto &row : m_backend.conversations()) if (row.toMap()["id"].toString() == m_targetSelected) {
            const auto ids = row.toMap()["message_ids"].toList();
            if (!ids.isEmpty()) m_targetMessage = ids.last().toString();
        }
        if (m_targetMessage.isEmpty() || m_targetSelected == m_backend.selectedId()) { finish("The clicked row must identify a different hydrated source conversation."); return; }
    }
    m_input["target_account"] = m_targetAccount; m_input["target_selected_id"] = m_targetSelected;
    m_modelAt = 0; m_visibleAt = 0; m_modelInvalidations = 0; m_visibilityWaits = 0;
    m_armedGeneration = 0; m_armedSnapshot = {}; m_paint->arm(0);
    m_phase = Phase::AwaitingModel; m_watchdog.start(10000); m_inspectTimer.start();
    m_inputUnixMs = QDateTime::currentMSecsSinceEpoch(); m_inputAt = GuiPaintObserver::nowNanos();
    if (m_class == 1 || m_class == 2) {
        const auto modifiers = m_class == 2 ? Qt::ShiftModifier : Qt::NoModifier;
        QKeyEvent overrideEvent(QEvent::ShortcutOverride, Qt::Key_Tab, modifiers);
        QKeyEvent press(QEvent::KeyPress, Qt::Key_Tab, modifiers), release(QEvent::KeyRelease, Qt::Key_Tab, modifiers);
        QCoreApplication::sendEvent(m_window, &overrideEvent);
        QCoreApplication::sendEvent(m_window, &press); QCoreApplication::sendEvent(m_window, &release);
    } else if (!click(control)) { finish("The actual navigation control could not receive native mouse input."); return; }
    inspect();
}
bool GuiInteractionBenchmark::modelCorrect() const {
    if (m_backend.account() != m_targetAccount || m_backend.conversations().isEmpty() || m_backend.selectedId().isEmpty()) return false;
    if (!m_targetSelected.isEmpty() && m_backend.selectedId() != m_targetSelected) return false;
    const auto conversation = m_backend.conversation();
    if (conversation["id"].toString() != m_backend.selectedId()) return false;
    const auto members = conversation["messages"].toList();
    if (members.isEmpty() || (!m_targetMessage.isEmpty() && members.last().toMap()["id"].toString() != m_targetMessage)) return false;
    bool found = false;
    for (const auto &row : m_backend.conversations()) if (row.toMap()["id"].toString() == m_backend.selectedId()) found = true;
    return found;
}
bool GuiInteractionBenchmark::visibleSnapshot(QJsonObject &snapshot) const {
    if (!modelCorrect() || !m_window->isExposed()) return false;
    const auto rows = m_backend.conversations();
    const auto ids = rowIds(rows);
    QJsonArray visibleRows;
    for (auto item : visibleItems(m_window, "conversationRow_")) {
        const auto id = item->objectName().mid(QStringLiteral("conversationRow_").size());
        if (!ids.contains(id) || visibleRows.contains(id)) return false;
        visibleRows.append(id);
    }
    if (visibleRows.isEmpty()) return false;
    const auto members = m_backend.conversation()["messages"].toList();
    const auto latest = members.last().toMap(); const auto mid = latest["id"].toString();
    auto member = findItem(m_window->contentItem(), "messageMember_" + mid);
    auto body = member ? bodyItem(member, latest["text"].toString(), m_window) : nullptr;
    if (!body || visibleRect(member, m_window).isEmpty()) return false;
    QJsonArray memberIds, visibleMembers;
    for (const auto &value : members) memberIds.append(value.toMap()["id"].toString());
    for (auto item : visibleItems(m_window, "messageMember_")) {
        const auto id = item->objectName().mid(QStringLiteral("messageMember_").size());
        if (!memberIds.contains(id) || visibleMembers.contains(id)) return false;
        visibleMembers.append(id);
    }
    const auto text = body->property("text").toString();
    snapshot = {{"account", m_backend.account()}, {"selected_id", m_backend.selectedId()}, {"row_ids", ids}, {"visible_row_ids", visibleRows},
        {"member_ids", memberIds}, {"visible_member_ids", visibleMembers}, {"message_id", mid},
        {"body_sha256", QString::fromLatin1(QCryptographicHash::hash(text.toUtf8(), QCryptographicHash::Sha256).toHex())},
        {"body_length", text.size()}, {"qml_visible_row_count", visibleRows.size()}, {"qml_visible_member_count", visibleMembers.size()},
        {"qml_body_type", body->metaObject()->className()}, {"qml_member_object", member->objectName()}};
    return true;
}
void GuiInteractionBenchmark::inspect() {
    if (m_phase != Phase::AwaitingModel && m_phase != Phase::AwaitingPaint) return;
    if (!modelCorrect()) {
        if (m_phase == Phase::AwaitingPaint) ++m_modelInvalidations;
        m_paint->arm(0); m_armedGeneration = 0; m_phase = Phase::AwaitingModel; return;
    }
    if (m_modelAt == 0) m_modelAt = GuiPaintObserver::nowNanos();
    QJsonObject snapshot;
    if (!visibleSnapshot(snapshot)) { ++m_visibilityWaits; return; }
    if (m_visibleAt == 0) m_visibleAt = GuiPaintObserver::nowNanos();
    if (m_phase == Phase::AwaitingPaint && snapshot == m_armedSnapshot) return;
    m_armedSnapshot = snapshot; m_armedGeneration = ++m_generation; m_phase = Phase::AwaitingPaint;
    m_paint->arm(m_armedGeneration); m_window->update();
}
void GuiInteractionBenchmark::painted(GuiPaintObserver::Frame frame) {
    if (m_phase != Phase::AwaitingPaint || !m_armedGeneration || frame.generation != m_armedGeneration || frame.serial <= m_lastSerial) return;
    QJsonObject snapshot;
    if (!visibleSnapshot(snapshot) || snapshot != m_armedSnapshot) { inspect(); return; }
    if (frame.swappedAt < m_modelAt || m_modelAt < m_inputAt) { finish("A pre-input or pre-model frame reached navigation measurement."); return; }
    m_lastSerial = frame.serial; m_watchdog.stop(); m_inspectTimer.stop(); m_paint->arm(0); m_armedGeneration = 0;
    if (m_preparingLargeMailbox) {
        m_preparingLargeMailbox = false; m_restoredSelections[m_backend.account()] = m_backend.selectedId();
        m_phase = Phase::BetweenSamples;
        QTimer::singleShot(25, this, [this] { beginSample(); });
        return;
    }
    const double total = double(frame.swappedAt - m_inputAt) / 1000000.0;
    for (auto it = m_input.begin(); it != m_input.end(); ++it) snapshot[it.key()] = it.value();
    snapshot["input_unix_ms"] = m_inputUnixMs; snapshot["paint_unix_ms"] = double(m_inputUnixMs) + total;
    snapshot["input_to_paint_ms"] = total; snapshot["input_to_model_ms"] = double(m_modelAt - m_inputAt) / 1000000.0;
    snapshot["model_to_paint_ms"] = double(frame.swappedAt - m_modelAt) / 1000000.0;
    snapshot["input_to_visible_ms"] = double(m_visibleAt - m_inputAt) / 1000000.0;
    snapshot["model_to_visible_ms"] = double(m_visibleAt - m_modelAt) / 1000000.0;
    snapshot["visible_to_paint_ms"] = double(frame.swappedAt - m_visibleAt) / 1000000.0;
    snapshot["model_invalidations"] = m_modelInvalidations; snapshot["visibility_waits"] = m_visibilityWaits;
    snapshot["frame_generation"] = qint64(frame.generation); snapshot["frame_serial"] = qint64(frame.serial);
    snapshot["loading_at_paint"] = m_backend.loading(); snapshot["read_mutation_in_flight_at_paint"] = m_backend.mutationInFlight();
    if (m_iteration > 0) { snapshot["sample"] = m_iteration; m_samples[m_class].append(snapshot); }
    m_restoredSelections[m_backend.account()] = m_backend.selectedId();
    if (++m_iteration > 100) {
        qInfo() << "Native interaction class completed:" << labels[m_class] << "100 measured correct content frames";
        m_iteration = 0; ++m_class;
    }
    if (m_class == m_routeCount) { finish(); return; }
    m_phase = Phase::BetweenSamples;
    QTimer::singleShot(25, this, [this] {
        if (m_class == 4 && m_iteration == 0) prepareLargeMailbox();
        else beginSample();
    });
}
QJsonObject GuiInteractionBenchmark::diagnostic() const {
    const auto conversation = m_backend.conversation();
    const auto members = conversation["messages"].toList();
    QJsonArray memberIds, rows, memberItems, bodies, queue;
    for (const auto &member : members) memberIds.append(member.toMap()["id"].toString());
    const auto latest = members.isEmpty() ? QVariantMap{} : members.last().toMap();
    collectDiagnostics(m_window->contentItem(), m_window, rows, memberItems, bodies);
    auto pendingJson = [this](const QString &id) {
        const auto found = m_backend.m_pending.constFind(id);
        if (found == m_backend.m_pending.cend()) return QJsonObject{};
        return QJsonObject{{"method", found->method}, {"account", found->account}, {"generation", found->generation},
            {"list_generation", found->listGeneration}, {"automatic_read", found->automaticRead}, {"query_bypasses", found->queryBypasses},
            {"started_unix_ms", found->started}};
    };
    for (const auto &request : m_backend.m_outgoing) queue.append(pendingJson(request.first));
    auto mail = findAnyItem(m_window->contentItem(), "mailWorkspace");
    QJsonObject mailState;
    for (const auto key : {"expanded", "replyOpen", "loadedConversation", "pendingReveal", "hydratedDraftIdentity", "restoring"})
        if (mail) mailState[key] = QJsonValue::fromVariant(mail->property(key));
    QJsonObject actualSnapshot;
    const bool visibleCorrect = visibleSnapshot(actualSnapshot);
    return {{"phase", int(m_phase)}, {"model_correct", modelCorrect()}, {"visible_snapshot_correct", visibleCorrect},
        {"account", m_backend.account()}, {"selected_id", m_backend.selectedId()}, {"conversation_id", conversation["id"].toString()},
        {"target_account", m_targetAccount}, {"target_selected_id", m_targetSelected}, {"target_message_id", m_targetMessage},
        {"model_row_ids", rowIds(m_backend.conversations())}, {"model_member_ids", memberIds}, {"latest_message_id", latest["id"].toString()},
        {"latest_body_length", latest["text"].toString().size()},
        {"latest_body_sha256", QString::fromLatin1(QCryptographicHash::hash(latest["text"].toString().toUtf8(), QCryptographicHash::Sha256).toHex())},
        {"list", itemDiagnostic(findAnyItem(m_window->contentItem(), "conversationList"), m_window)},
        {"reader", itemDiagnostic(findAnyItem(m_window->contentItem(), "reader"), m_window)}, {"mail", mailState},
        {"row_items", rows}, {"member_items", memberItems}, {"body_items", bodies}, {"actual_visible_snapshot", actualSnapshot},
        {"armed_snapshot", m_armedSnapshot}, {"armed_generation", qint64(m_armedGeneration)},
        {"model_elapsed_ms", m_modelAt ? double(m_modelAt - m_inputAt) / 1000000.0 : -1},
        {"visible_elapsed_ms", m_visibleAt ? double(m_visibleAt - m_inputAt) / 1000000.0 : -1},
        {"model_invalidations", m_modelInvalidations}, {"visibility_waits", m_visibilityWaits},
        {"backend_generation", m_backend.m_generation}, {"backend_list_generation", m_backend.m_listGeneration},
        {"pinned_index", m_backend.m_pinnedIndex}, {"pinned_row", QJsonObject::fromVariantMap(m_backend.m_pinnedRow)},
        {"in_flight", pendingJson(m_backend.m_inFlight)}, {"queued", queue}};
}
void GuiInteractionBenchmark::finish(const QString &error) {
    if (m_phase == Phase::Finished) return;
    const int interruptedClass = m_class, interruptedSample = m_iteration;
    auto interruption = error.isEmpty() ? QJsonObject{} : diagnostic();
    stopForShutdown();
    // Readback is deliberately after the timed observation has stopped. The
    // failure PNG diagnoses geometry and is never used as a latency sample.
    if (!error.isEmpty()) {
        const auto path = QFileInfo(m_output).absolutePath() + "/" + QFileInfo(m_output).completeBaseName() + "-failure.png";
        const auto capture = m_window->grabWindow();
        interruption["failure_png"] = path; interruption["failure_png_saved"] = !capture.isNull() && capture.save(path);
    }
    QJsonArray classes; bool passed = error.isEmpty();
    for (int index = 0; index < m_routeCount; ++index) {
        auto row = summary(m_samples[index], "input_to_paint_ms");
        row["class"] = labels[index]; row["observations"] = m_samples[index].size(); row["samples"] = m_samples[index];
        row["input_to_model"] = summary(m_samples[index], "input_to_model_ms");
        row["model_to_paint"] = summary(m_samples[index], "model_to_paint_ms");
        row["model_to_visible"] = summary(m_samples[index], "model_to_visible_ms");
        row["visible_to_paint"] = summary(m_samples[index], "visible_to_paint_ms");
        const bool classPassed = m_samples[index].size() == 100 && row["p95_ms"].toDouble(1000) <= 100;
        row["budget_passed"] = classPassed; passed &= classPassed; classes.append(row);
    }
    m_budgetPassed = passed;
    const QJsonObject report{{"result", error.isEmpty() ? "measured" : "failed"}, {"error", error}, {"classes", classes}, {"budget_passed", passed}, {"p95_budget_ms", 100},
        {"interrupted_class", interruptedClass}, {"interrupted_sample", interruptedSample}, {"interrupted_input", m_input},
        {"interruption_diagnostic", interruption},
        {"boundary", "Native QQuickWindow mouse/key event to the first frame carrying the matching account, conversation model, actual visible row delegates and latest-message body. Generation captured afterSynchronizing, carried through afterRendering and directly timestamped frameSwapped."},
        {"sample_policy", "One unmeasured warmup per route; 100 measured inputs per route; next input 25 ms after the prior correct paint. Normal polling, read-on-open and serial backend queues remain enabled. No pending-queue drain."},
        {"identity_oracle", "Every sampled model row, visible delegate, selected conversation and latest body hash is recorded for independent SQLite account/source verification."},
        {"accounts", QJsonArray::fromStringList(m_accounts)}, {"host", QSysInfo::prettyProductName()}, {"architecture", QSysInfo::currentCpuArchitecture()},
        {"large_mailbox_account", m_routeCount == 5 ? m_largeMailbox : QString{}}, {"large_mailbox_conversation_count", m_routeCount == 5 ? m_largeMailboxCount : 0},
        {"qt_version", QT_VERSION_STR}, {"renderer_api", int(m_window->rendererInterface()->graphicsApi())},
        {"window_width", m_window->width()}, {"window_height", m_window->height()}, {"device_pixel_ratio", m_window->devicePixelRatio()},
        {"synchronizations", qint64(m_paint->syncCount())}, {"renders", qint64(m_paint->renderCount())}, {"swaps", qint64(m_paint->swapCount())}};
    QSaveFile output(m_output);
    const auto bytes = QJsonDocument(report).toJson();
    if (!output.open(QIODevice::WriteOnly) || output.write(bytes) != bytes.size() || !output.commit()) {
        m_budgetPassed = false; qWarning() << "Could not write navigation benchmark report:" << m_output;
    }
    if (!error.isEmpty()) qWarning() << "Native interaction benchmark failed:" << error;
    m_backend.shutdown();
}
