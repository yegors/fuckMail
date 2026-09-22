#include "DebugConsole.h"
#include <QClipboard>
#include <QDateTime>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QSysInfo>
#include <QUuid>
#include <cstdio>
#include <QCryptographicHash>
#include <QFile>

namespace {
QMutex handlerMutex;
DebugConsole *sink = nullptr;
QtMessageHandler previousHandler = nullptr;
void messageHandler(QtMsgType type, const QMessageLogContext &context, const QString &message) {
    QtMessageHandler previous;
    {
        QMutexLocker lock(&handlerMutex);
        previous = previousHandler;
        if (sink) sink->record(type == QtCriticalMsg || type == QtFatalMsg ? "ERROR" : type == QtWarningMsg ? "WARN" : type == QtDebugMsg ? "DEBUG" : "INFO",
            "Qt", message, {{"category", QString::fromUtf8(context.category ? context.category : "default")}});
    }
    if (previous) previous(type, context, message);
    else std::fprintf(stderr, "%s\n", qFormatLogMessage(type, context, message).toLocal8Bit().constData());
}
QString json(const QVariantMap &map, bool pretty = false) {
    return QString::fromUtf8(QJsonDocument::fromVariant(map).toJson(pretty ? QJsonDocument::Indented : QJsonDocument::Compact)).trimmed();
}
// Explicit projections keep future payload additions out of reports by default.
QVariantMap pick(const QVariantMap &input, std::initializer_list<const char *> keys) {
    QVariantMap out;
    for (const auto key : keys) if (input.contains(key)) {
        const auto value = input.value(key);
        if (value.metaType().id() == QMetaType::QString) out[key] = DebugConsole::scrub(value.toString());
        else if (value.metaType().id() != QMetaType::QVariantMap && value.metaType().id() != QMetaType::QVariantList) out[key] = value;
    }
    return out;
}
}

DebugConsole::DebugConsole(QObject *parent) : QObject(parent), m_session(QUuid::createUuid().toString(QUuid::WithoutBraces)) {
    m_uptime.start();
    m_timer.setInterval(500);
    connect(&m_timer, &QTimer::timeout, this, &DebugConsole::refresh);
    m_timer.start();
    record("INFO", "app", "Debug capture started; bounded in-memory session history.");
}
DebugConsole::~DebugConsole() {
    if (m_capturing) {
        QMutexLocker lock(&handlerMutex);
        sink = nullptr;
        qInstallMessageHandler(previousHandler);
    }
}
void DebugConsole::captureQtMessages() {
    QMutexLocker lock(&handlerMutex);
    if (sink || m_capturing) return;
    sink = this; m_capturing = true;
    previousHandler = qInstallMessageHandler(messageHandler);
}
void DebugConsole::attach(QObject *backend) {
    m_backend = backend;
    connect(backend, SIGNAL(debugEvent(QString,QString,QString,QVariantMap)), this, SLOT(record(QString,QString,QString,QVariantMap)));
    connect(backend, SIGNAL(errorChanged()), this, SLOT(refresh()));
    connect(backend, SIGNAL(connectedChanged()), this, SLOT(refresh()));
    connect(backend, SIGNAL(googleSyncChanged()), this, SLOT(refresh()));
    connect(backend, SIGNAL(assistanceReply(QString,QString,QVariantMap,bool)), this, SLOT(assistanceReply(QString,QString,QVariantMap,bool)));
    refresh();
}
void DebugConsole::identifyRuntime(const QString &engine) {
    const auto digest = [](const QString &path) {
        QFile file(path); QCryptographicHash hash(QCryptographicHash::Sha256);
        return file.open(QIODevice::ReadOnly) && hash.addData(&file) ? QString::fromLatin1(hash.result().toHex()) : QString("unavailable");
    };
    m_identity = {{"GUI_SHA256", digest(QCoreApplication::applicationFilePath())}, {"configured_engine_file_SHA256", digest(engine)}};
}
void DebugConsole::assistanceReply(const QString &, const QString &method, const QVariantMap &result, bool ok) {
    if (!ok || method != "assist_job") return; // IPC failures are already recorded.
    auto safe = pick(result, {"status", "task"});
    const auto error = pick(result.value("error").toMap(), {"code", "message"});
    safe["error"] = error;
    const auto job = result.value("job_id").toString();
    if (m_jobs.value(job) == safe) return;
    if (!m_jobs.contains(job) && m_jobs.size() >= 64) m_jobs.erase(m_jobs.begin());
    m_jobs[job] = safe; m_assistance = safe;
    record(error.isEmpty() ? "INFO" : "ERROR", "local model", "Job " + safe.value("status").toString() + " " + error.value("message").toString(), {{"code", error.value("code")}});
}
QString DebugConsole::scrub(QString text) {
    // Scrub before retention, not just when copying. Never use this as a license
    // to log arbitrary payloads: only metadata and existing diagnostic strings.
    static const QRegularExpression secret(R"((?i)(bearer\s+)[^\s,;]+|["']?(?:access_token|refresh_token|id_token|client_secret|authorization|password|token)["']?\s*[:=]\s*(?:"[^"]*"|'[^']*'|[^\s,;}]+))");
    static const QRegularExpression url(R"((?:https?|file)://[^\s<>"']+)", QRegularExpression::CaseInsensitiveOption);
    static const QRegularExpression email(R"([\w.!#$%&'*+/=?^`{|}~-]+@[\w.-]+)");
    static const QRegularExpression path(R"((?:[A-Za-z]:[\\/]|/(?:Users|home|tmp|var)/)[^\r\n"<>|]*)");
    static const QRegularExpression quoted(R"("[^"\r\n]*"|'[^'\r\n]*')");
    static const QRegularExpression opaque(R"(\b[A-Za-z0-9_\-./+=]{40,}\b)");
    text = text.left(8192);
    text.replace(secret, "[secret]"); text.replace(url, "[url]"); text.replace(email, "[email]");
    text.replace(path, "[path]"); text.replace(quoted, "[quoted text]"); text.replace(opaque, "[opaque value]");
    text.replace(QRegularExpression("[\\x00-\\x1f\\x7f]+"), " ");
    return text.size() > 1024 ? text.left(1024) + " [truncated]" : text;
}
void DebugConsole::record(const QString &level, const QString &source, const QString &message, const QVariantMap &fields) {
    const auto safe = pick(fields, {"method", "phase", "code", "elapsed_ms", "retry_after_secs", "queued", "category"});
    const auto severity = QStringList{"DEBUG", "INFO", "WARN", "ERROR"}.contains(level) ? level : "INFO";
    const auto line = QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs) + "  " + severity + "  "
        + scrub(source).left(40) + "  " + scrub(message) + (safe.isEmpty() ? QString{} : "  " + json(safe));
    QMutexLocker lock(&m_mutex);
    if (m_entries.size() >= 1000) { m_entries.pop_front(); ++m_dropped; }
    m_entries.push_back({severity, line.left(2048)}); m_dirty = true;
}
QString DebugConsole::accountAlias(const QString &account) {
    if (account.isEmpty()) return "none";
    if (!m_aliases.contains(account)) m_aliases[account] = "account-" + QString::number(m_aliases.size() + 1);
    return m_aliases.value(account);
}
QVariantMap DebugConsole::snapshot() {
    QVariantMap state{{"app", "fuckMail"}, {"version", QCoreApplication::applicationVersion()}, {"Qt", qVersion()},
        {"OS", QSysInfo::prettyProductName()}, {"architecture", QSysInfo::currentCpuArchitecture()}, {"session", m_session}};
    state["binaries"] = m_identity;
    state["last_observed_model_job"] = m_assistance;
    if (!m_backend) return state;
    const auto get = [this](const char *name) { return m_backend->property(name); };
    state["engine"] = pick(get("diagnostics").toMap(), {"schema_version", "fixture_revision", "revision", "calendar_generation", "synthetic", "profile_kind"});
    state["IPC"] = pick(get("debugRuntime").toMap(), {"pending", "queued", "in_flight", "provider_operations", "engine_pid", "shutting_down"});
    QVariantMap ui;
    for (const auto name : {"connected", "startupReady", "loading", "mutationInFlight", "preferencesPending", "calendarReady", "timelineReady", "draftBusy", "draftLocked", "draftConflict", "sendQueued", "sendingCount"}) ui[name] = get(name);
    ui["selected_account"] = accountAlias(get("account").toString());
    ui["mail_search_active"] = !get("query").toString().isEmpty();
    ui["calendar_search_active"] = !get("calendarSearchQuery").toString().isEmpty();
    ui["conversation_selected"] = !get("selectedId").toString().isEmpty();
    ui["draft_open"] = !get("activeDraftId").toString().isEmpty();
    ui["error"] = scrub(get("error").toString());
    state["UI"] = ui;
    QVariantMap accounts;
    for (const auto &item : get("accounts").toList()) {
        const auto row = item.toMap();
        auto safe = pick(row, {"unread_count", "starred_count", "message_count", "conversation_count"});
        safe["coverage"] = pick(row.value("coverage").toMap(), {"synthetic", "inventory_complete", "delta_current", "messages", "indexed_messages", "text_index_complete", "retained_sources", "messages_with_complete_attachments", "unavailable_messages", "offline_complete"});
        safe["google"] = pick(row.value("google").toMap(), {"connection_state"});
        accounts[accountAlias(row.value("id").toString())] = safe;
    }
    state["accounts"] = accounts;
    state["auth"] = pick(get("googleAuth").toMap(), {"phase", "error_code", "message"});
    state["delivery"] = pick(get("mailDelivery").toMap(), {"state", "total"});
    QVariantMap sync;
    const auto rawSync = get("googleSync").toMap();
    for (auto it = rawSync.cbegin(); it != rawSync.cend(); ++it) {
        const auto row = it.value().toMap();
        auto safe = pick(row, {"active", "owned", "credential_access_required", "last_mail_sync_at", "refresh_requested"});
        safe["progress"] = pick(row.value("progress").toMap(), {"mail", "mail_error", "mail_steps", "mail_mutations", "mail_mutations_error", "drafts", "drafts_error", "remote_drafts", "remote_drafts_error", "labels", "labels_error", "calendar", "calendar_error", "calendar_pages", "calendar_failures", "calendars", "calendar_catalog", "calendar_events", "calendar_free_busy"});
        sync[accountAlias(it.key())] = safe;
    }
    state["sync"] = sync;
    state["quota_limits"] = pick(get("googleSyncLimits").toMap(), {"gmail_project_units_per_minute", "gmail_account_units_per_minute", "calendar_project_units_per_minute", "calendar_account_units_per_minute", "max_in_flight"});
    return state;
}
void DebugConsole::refresh() {
    auto next = snapshot();
    const auto sync = next.value("sync").toMap();
    for (auto it = sync.cbegin(); it != sync.cend(); ++it) if (m_lastSync.value(it.key()) != it.value()) {
        // Values already passed through the explicit projection; don't scrub JSON
        // punctuation away. Emit each changed field as a readable state change.
        const auto row = it.value().toMap();
        const auto old = m_lastSync.value(it.key()).toMap();
        for (auto field = row.cbegin(); field != row.cend(); ++field) if (old.value(field.key()) != field.value()) {
            if (field.key() == "progress") {
                const auto progress = field.value().toMap();
                const auto before = old.value("progress").toMap();
                for (auto p = progress.cbegin(); p != progress.cend(); ++p) if (before.value(p.key()) != p.value())
                    record(p.key().endsWith("_error") && !p.value().toString().isEmpty() ? "WARN" : "INFO", "sync",
                        it.key() + " " + p.key() + " = " + p.value().toString());
            } else record("INFO", "sync", it.key() + " " + field.key() + " = " + field.value().toString());
        }
    }
    for (auto it = m_lastSync.cbegin(); it != m_lastSync.cend(); ++it) if (!sync.contains(it.key())) record("INFO", "sync", it.key() + " no longer in sync snapshot");
    m_lastSync = sync;
    const auto ui = next.value("UI").toMap(), before = m_state.value("UI").toMap();
    for (const auto key : {"connected", "selected_account", "error"}) if (ui.value(key) != before.value(key))
        record(QString(key) == "error" && !ui.value(key).toString().isEmpty() ? "ERROR" : "INFO", "app", QString(key) + " = " + ui.value(key).toString());
    if (next.value("auth") != m_state.value("auth")) {
        const auto auth = next.value("auth").toMap();
        record(auth.value("error_code").toString().isEmpty() ? "INFO" : "WARN", "auth", auth.value("phase").toString() + " " + auth.value("message").toString(), {{"code", auth.value("error_code")}});
    }
    const bool stateChanged = next != m_state;
    m_state = next;
    bool dirty;
    { QMutexLocker lock(&m_mutex); dirty = m_dirty; m_dirty = false; }
    if (stateChanged || dirty) emit changed();
}
QString DebugConsole::stateText() const { return json(m_state, true); }
int DebugConsole::count() const { QMutexLocker lock(&m_mutex); return int(m_entries.size()); }
qulonglong DebugConsole::dropped() const { QMutexLocker lock(&m_mutex); return m_dropped; }
QString DebugConsole::events(const QString &level, const QString &filter) const {
    QMutexLocker lock(&m_mutex);
    QStringList lines;
    for (const auto &entry : m_entries)
        if ((level.isEmpty() || level == "All" || entry.level == level || (level == "Warnings + errors" && (entry.level == "WARN" || entry.level == "ERROR")))
            && (filter.isEmpty() || entry.line.contains(filter, Qt::CaseInsensitive))) lines.append(entry.line);
    return lines.join('\n');
}
QString DebugConsole::report() const {
    return "fuckMail debug report v1\nCaptured: " + QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs)
        + "\nSession uptime: " + QString::number(m_uptime.elapsed() / 1000) + " seconds\n"
        + "Session only; latest 1000 events; " + QString::number(dropped()) + " older events evicted.\n"
        + "Account aliases; payloads omitted; diagnostic strings redacted.\n\nRUNTIME STATE\n" + stateText()
        + "\n\nEVENTS (all levels, oldest first)\n" + events() + '\n';
}
void DebugConsole::copyReport() const { if (auto clipboard = QGuiApplication::clipboard()) clipboard->setText(report()); }
void DebugConsole::clear() {
    { QMutexLocker lock(&m_mutex); m_entries.clear(); m_dropped = 0; }
    record("INFO", "app", "Event history cleared by user.");
    refresh();
}
