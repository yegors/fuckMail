#include "DebugConsole.h"
#include <QClipboard>
#include <QDateTime>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSysInfo>
#include <QUuid>
#include <cstdio>
#include <QCryptographicHash>
#include <QFile>
#include <QDir>
#include <QFileInfo>

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
constexpr qint64 logSegmentBytes = 2560 * 1024; // GUI and engine, current and previous: 10 MiB total.
QString groupedScope(const QString &scope) {
    if (QStringList{"Mail", "Drafts", "Sending", "Search"}.contains(scope)) return "Mail";
    if (QStringList{"Sync", "Auth", "Accounts"}.contains(scope)) return "Sync";
    if (scope == "Calendar" || scope == "Model") return scope;
    return "App";
}
QString scopeFor(const QString &source, const QVariantMap &fields) {
    if (source.compare("IPC", Qt::CaseInsensitive) == 0) {
        const auto method = fields.value("method").toString();
        if (method.startsWith("assist_")) return "Model";
        if (method.startsWith("calendar_") || method.startsWith("google_calendar_")) return "Calendar";
        if (method.startsWith("google_auth_") || method == "google_disconnect" || method.startsWith("account_") || method == "accounts") return "Sync";
        if (method.startsWith("google_sync_") || method.startsWith("google_labels") || method == "google_mail_mutation_status") return "Sync";
        if (method.startsWith("google_draft") || method.startsWith("draft_") || method == "drafts" || method == "capture_history") return "Mail";
        if (method.startsWith("google_send") || method.startsWith("google_sent") || method.startsWith("send_") || method.startsWith("mail_")) return "Mail";
        if (method.startsWith("search_")) return "Mail";
        if (method.startsWith("conversation") || method.startsWith("message_") || method.startsWith("attachment")
            || method.startsWith("recipient_") || method.startsWith("sender_") || method.startsWith("category_")
            || method.startsWith("google_category_") || method.startsWith("link_") || method == "remote_image_cache" || method == "categories") return "Mail";
        return "App";
    }
    if (source.compare("local model", Qt::CaseInsensitive) == 0 || source.compare("model", Qt::CaseInsensitive) == 0) return "Model";
    if (source.compare("sync", Qt::CaseInsensitive) == 0 || source.compare("auth", Qt::CaseInsensitive) == 0) return "Sync";
    return groupedScope(source);
}
void writeBounded(const QString &directory, const QString &name, const QByteArray &line) {
    if (directory.isEmpty()) return;
    const auto path = QDir(directory).filePath(name);
    QFile file(path);
    if (file.size() + line.size() > logSegmentBytes) {
        QFile::remove(path + ".1");
        if (!QFile::rename(path, path + ".1")) file.resize(0);
    }
    if (file.open(QIODevice::WriteOnly | QIODevice::Append)) { file.write(line); file.close(); }
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
    record("INFO", "app", "Debug capture started.");
}
void DebugConsole::setLogDirectory(const QString &profile) {
    m_profile = profile;
    // A new profile must first be created and validated by fm-engine. On Unix,
    // creating it here with QDir would give it shared default permissions.
    if (!m_backend && QDir(profile).exists()) activateLogDirectory();
    else if (m_backend && m_backend->property("connected").toBool()) activateLogDirectory();
}
void DebugConsole::activateLogDirectory() {
    if (m_profile.isEmpty() || !m_logDirectory.isEmpty()) return;
    const auto path = QDir(m_profile).filePath("diagnostics");
    if (!QDir().mkpath(path)) return;
    QMutexLocker lock(&m_mutex);
    m_logDirectory = path;
    // The first capture is also persisted if the profile became available later.
    for (const auto &entry : m_entries) writeBounded(path, "gui.log", (entry.line + '\n').toUtf8());
    m_dirty = true;
}
QString DebugConsole::logDirectory() const { QMutexLocker lock(&m_mutex); return m_logDirectory; }
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
    const auto safe = pick(fields, {"method", "request_ref", "phase", "code", "elapsed_ms", "retry_after_secs", "queued", "category", "status", "task", "completed", "total", "failed"});
    const auto severity = QStringList{"DEBUG", "INFO", "WARN", "ERROR"}.contains(level) ? level : "INFO";
    const auto scope = scopeFor(source, safe);
    append(severity, scope, QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs), scrub(source).left(40), scrub(message), safe, true);
}
void DebugConsole::append(const QString &level, const QString &scope, const QString &at, const QString &source,
    const QString &message, const QVariantMap &fields, bool persist) {
    const auto detail = fields.isEmpty() ? QString{} : json(fields);
    const auto line = (at + "  " + level + "  " + scope + "  " + source + "  " + message
        + (detail.isEmpty() ? QString{} : "  " + detail)).left(2048);
    QMutexLocker lock(&m_mutex);
    if (m_entries.size() >= 1000) { m_entries.pop_front(); ++m_dropped; }
    m_entries.push_back({level, scope, line, {{"sequence", QString::number(++m_sequence)}, {"at", at},
        {"level", level}, {"scope", scope}, {"source", source}, {"message", message}, {"details", detail}, {"line", line}}});
    m_dirty = true;
    if (persist) writeBounded(m_logDirectory, "gui.log", (line + '\n').toUtf8());
}
void DebugConsole::readEngineLog() {
    QString directory;
    { QMutexLocker lock(&m_mutex); directory = m_logDirectory; }
    if (directory.isEmpty()) return;
    const auto path = QDir(directory).filePath("engine.log");
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) return;
    const auto birth = QFileInfo(path).birthTime();
    if (file.size() < m_engineOffset || (birth.isValid() && m_engineBirthTime.isValid() && birth != m_engineBirthTime)) {
        m_engineOffset = 0; m_enginePartial.clear();
    }
    m_engineBirthTime = birth;
    if (!file.seek(m_engineOffset)) return;
    const auto bytes = file.read(256 * 1024);
    m_engineOffset += bytes.size();
    m_enginePartial += bytes;
    while (true) {
        const auto end = m_enginePartial.indexOf('\n');
        if (end < 0) break;
        const auto line = m_enginePartial.left(end);
        m_enginePartial.remove(0, end + 1);
        QJsonParseError error;
        const auto event = QJsonDocument::fromJson(line, &error).object();
        const auto rawScope = event.value("scope").toString();
        if (error.error != QJsonParseError::NoError || !QStringList{"App", "IPC", "Sync", "Auth", "Model", "Calendar", "Mail", "Drafts", "Sending", "Accounts", "Settings", "Search", "Other"}.contains(rawScope)) continue;
        const auto scope = groupedScope(rawScope);
        const auto rawLevel = event.value("level").toString();
        const auto level = QStringList{"DEBUG", "INFO", "WARN", "ERROR"}.contains(rawLevel) ? rawLevel : "INFO";
        const auto rawFields = event.value("fields").toObject().toVariantMap();
        auto fields = pick(rawFields, {"job_ref", "account_ref", "target_ref", "task", "origin", "provider", "model", "model_digest", "prompt_sha256", "label_state", "input_chars", "summary_chars", "sender_kind",
            "category_abstain", "partial", "source_count", "label_choices", "elapsed_ms", "code", "attempt",
            "completed", "total", "service", "state", "connection_generation", "generation", "lane", "items",
            "normalized_bytes", "page_complete", "catalog_refresh", "calendars", "pages", "failures", "more",
            "first_error_code", "pid"});
        QVariantList categoryIds;
        for (const auto &id : rawFields.value("category_ids").toList().mid(0, 3)) categoryIds.append(scrub(id.toString()).left(80));
        if (!categoryIds.isEmpty()) fields["category_ids"] = categoryIds;
        append(level, scope, scrub(event.value("at").toString()), "engine", scrub(event.value("event").toString()), fields, false);
    }
    if (m_enginePartial.size() > 4096) m_enginePartial.clear();
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
    if (m_backend && m_backend->property("connected").toBool()) activateLogDirectory();
    readEngineLog();
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
QString DebugConsole::events(const QString &level, const QString &filter, const QString &scope) const {
    QStringList lines;
    for (const auto &row : eventRows(level, filter, scope)) lines.append(row.toMap().value("line").toString());
    return lines.join('\n');
}
QVariantList DebugConsole::eventRows(const QString &level, const QString &filter, const QString &scope) const {
    QMutexLocker lock(&m_mutex);
    QVariantList rows;
    for (const auto &entry : m_entries)
        if ((scope.isEmpty() || scope == "All" || entry.scope == scope)
            && (level.isEmpty() || level == "All" || entry.level == level || (level == "Warnings + errors" && (entry.level == "WARN" || entry.level == "ERROR")))
            && (filter.isEmpty() || entry.line.contains(filter, Qt::CaseInsensitive))) rows.append(entry.row);
    return rows;
}
QString DebugConsole::report() const {
    return "fuckMail debug report v1\nCaptured: " + QDateTime::currentDateTimeUtc().toString(Qt::ISODateWithMs)
        + "\nSession uptime: " + QString::number(m_uptime.elapsed() / 1000) + " seconds\n"
        + "Latest 1000 in-memory events; " + QString::number(dropped()) + " older events evicted.\n"
        + "Bounded disk logs in the active profile (gui.log and engine.log, each with one previous segment).\n"
        + "Account aliases; payloads omitted; diagnostic strings redacted.\n\nRUNTIME STATE\n" + stateText()
        + "\n\nEVENTS (all levels, oldest first)\n" + events() + '\n';
}
void DebugConsole::copyReport() const { if (auto clipboard = QGuiApplication::clipboard()) clipboard->setText(report()); }
void DebugConsole::clear() {
    { QMutexLocker lock(&m_mutex); m_entries.clear(); m_dropped = 0; }
    record("INFO", "app", "Event view cleared by user; disk logs retained.");
    refresh();
}
