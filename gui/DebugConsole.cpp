#include "DebugConsole.h"
#include <QClipboard>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QStandardPaths>
#include <cstdio>

namespace {
QMutex handlerMutex;
DebugConsole *sink = nullptr;
QtMessageHandler previousHandler = nullptr;
const QStringList levelNames{"emerg", "alert", "crit", "err", "warning", "notice", "info", "debug"};
const QStringList facilityNames{"sync", "gmail", "cal", "llm", "db", "net", "ui", "app"};

void messageHandler(QtMsgType type, const QMessageLogContext &context, const QString &message) {
    QtMessageHandler previous;
    {
        QMutexLocker lock(&handlerMutex);
        previous = previousHandler;
        const int level = type == QtFatalMsg ? 2 : type == QtCriticalMsg ? 3 : type == QtWarningMsg ? 4 : type == QtInfoMsg ? 6 : 7;
        if (sink) sink->record(levelNames[level], "Qt", message, {{"category", QString::fromUtf8(context.category ? context.category : "default")}});
    }
    if (previous) previous(type, context, message);
    else std::fprintf(stderr, "%s\n", qFormatLogMessage(type, context, message).toLocal8Bit().constData());
}
constexpr qint64 logSegmentBytes = 2560 * 1024; // GUI and engine, current and previous: 10 MiB total.
constexpr int jsonLimit = 16 * 1024;
QString facilityForMethod(const QString &m) {
    if (m.startsWith("assist_")) return "llm";
    if (m.startsWith("calendar_") || m.startsWith("google_calendar_")) return "cal";
    if (m.startsWith("google_auth_") || m == "google_disconnect" || m.startsWith("account_") || m == "accounts"
        || m.startsWith("google_sync_") || m.startsWith("google_labels") || m == "google_mail_mutation_status") return "sync";
    if (m.startsWith("google_")) return "gmail";
    if (m.startsWith("remote_") || m.startsWith("link_") || m == "message_html") return "net";
    if (m.startsWith("search_") || m.startsWith("conversation") || m.startsWith("message_") || m.startsWith("draft")
        || m == "capture_history" || m.startsWith("attachment") || m.startsWith("recipient_") || m.startsWith("sender_")
        || m.startsWith("category_") || m == "categories" || m.startsWith("mail_") || m.startsWith("send_")
        || m.startsWith("label_") || m.startsWith("preferences_") || m.startsWith("view_")) return "db";
    return "app";
}
QString facilityFor(const QString &source, const QVariantMap &fields) {
    const auto method = fields.value("method").toString();
    if (!method.isEmpty()) return facilityForMethod(method);
    const auto s = source.toLower();
    if (s == "qt") {
        const auto category = fields.value("category").toString();
        return category.startsWith("qt.") || category.startsWith("qml") || category == "js" ? "ui" : "app";
    }
    if (s == "local model" || s == "model") return "llm";
    if (s == "sync" || s == "auth" || s == "accounts") return "sync";
    if (s == "calendar") return "cal";
    if (facilityNames.contains(s)) return s;
    return "app";
}
// The engine's own scopes.
QString facilityForScope(const QString &scope) {
    if (scope == "Model") return "llm";
    if (scope == "Calendar") return "cal";
    if (scope == "Sync" || scope == "Auth" || scope == "Accounts") return "sync";
    if (scope == "Mail" || scope == "Drafts" || scope == "Sending") return "gmail";
    if (scope == "Search") return "db";
    return "app";
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
// Explicit projections keep future payload additions out of logs by default.
QVariantMap pick(const QVariantMap &input, std::initializer_list<const char *> keys) {
    QVariantMap out;
    for (const auto key : keys) if (input.contains(key)) {
        const auto value = input.value(key);
        if (value.metaType().id() == QMetaType::QString) out[key] = DebugConsole::scrub(value.toString());
        else if (value.metaType().id() != QMetaType::QVariantMap && value.metaType().id() != QMetaType::QVariantList) out[key] = value;
    }
    return out;
}
// A structured payload: strings scrubbed like messages, bounded in depth and width.
QVariant scrubTree(const QVariant &value, int depth) {
    const auto type = value.metaType().id();
    if (type == QMetaType::QVariantMap) {
        if (depth >= 6) return QStringLiteral("[…]");
        QVariantMap out; int n = 0;
        const auto map = value.toMap();
        for (auto it = map.cbegin(); it != map.cend() && n < 64; ++it, ++n) out[it.key().left(64)] = scrubTree(it.value(), depth + 1);
        return out;
    }
    if (type == QMetaType::QVariantList || type == QMetaType::QStringList) {
        if (depth >= 6) return QStringLiteral("[…]");
        QVariantList out;
        for (const auto &item : value.toList().mid(0, 50)) out.append(scrubTree(item, depth + 1));
        return out;
    }
    if (type == QMetaType::QString) return DebugConsole::scrub(value.toString());
    return value;
}
QString contextOf(const QVariantMap &fields) {
    QStringList parts;
    for (auto it = fields.cbegin(); it != fields.cend(); ++it) {
        auto text = it.value().toString();
        if (it.value().metaType().id() == QMetaType::Bool) text = it.value().toBool() ? "true" : "false";
        parts.append(it.key() + '=' + text);
    }
    return parts.join(' ');
}
void setJson(DebugLogEntry &entry, const QVariant &payload) {
    if (!payload.isValid() || payload.isNull()) return;
    const auto document = QJsonDocument::fromVariant(scrubTree(payload, 0));
    if (document.isNull()) return;
    const auto compact = QString::fromUtf8(document.toJson(QJsonDocument::Compact));
    if (compact.size() > jsonLimit) { entry.context += (entry.context.isEmpty() ? "" : " ") + QStringLiteral("json=omitted_too_large"); return; }
    entry.json = compact;
    entry.jsonLines = int(document.toJson(QJsonDocument::Indented).trimmed().count('\n')) + 1;
}
QString pad(const QString &text, int width) { return text.leftJustified(width, ' '); }
QString fileLine(const DebugLogEntry &e) {
    const auto facility = e.console ? QStringLiteral("console") : e.facility;
    return QDateTime::fromMSecsSinceEpoch(e.at, QTimeZone::UTC).toString(Qt::ISODateWithMs) + " <" + QString::number(e.level) + "> "
        + pad(levelNames.value(e.level), 7) + ' ' + pad(facility, 7) + ' ' + e.message
        + (e.context.isEmpty() ? QString{} : "  {" + e.context + '}');
}
QString bytesText(qint64 bytes) {
    if (bytes < 0) return {};
    if (bytes < 1024) return QString::number(bytes) + " B";
    if (bytes < 1024 * 1024) return QString::number(bytes / 1024.0, 'f', 0) + " KB";
    if (bytes < 1024ll * 1024 * 1024) return QString::number(bytes / (1024.0 * 1024), 'f', 1) + " MB";
    return QString::number(bytes / (1024.0 * 1024 * 1024), 'f', 2) + " GB";
}
QString homeRelative(const QString &path) {
    const auto home = QDir::homePath();
    const auto clean = QDir::cleanPath(path);
    return clean.startsWith(home + '/') ? "~" + clean.mid(home.size()) : QDir::toNativeSeparators(clean);
}
}

QVariant DebugLogModel::data(const QModelIndex &index, int role) const {
    if (!index.isValid() || index.row() >= int(m_rows.size())) return {};
    const auto &e = m_rows[size_t(index.row())];
    switch (role) {
    case SequenceRole: return QString::number(e.sequence);
    case TimeRole: return QDateTime::fromMSecsSinceEpoch(e.at).toString("HH:mm:ss.zzz");
    case LevelRole: return e.level;
    case FacilityRole: return e.console ? QString{} : e.facility;
    case MessageRole: return e.message;
    case ContextRole: return e.context;
    case JsonRole: return e.json;
    case JsonLinesRole: return e.jsonLines;
    case ConsoleRole: return e.console;
    default: return {};
    }
}
QHash<int, QByteArray> DebugLogModel::roleNames() const {
    return {{SequenceRole, "sequence"}, {TimeRole, "time"}, {LevelRole, "level"}, {FacilityRole, "facility"},
        {MessageRole, "message"}, {ContextRole, "context"}, {JsonRole, "json"}, {JsonLinesRole, "jsonLines"}, {ConsoleRole, "typed"}};
}

DebugConsole::DebugConsole(QObject *parent) : QObject(parent), m_model(this) {
    m_uptime.start();
    m_timer.setInterval(500);
    connect(&m_timer, &QTimer::timeout, this, &DebugConsole::refresh);
    m_timer.start();
    record("info", "app", "Debug capture started.");
}
DebugConsole::~DebugConsole() {
    if (m_capturing) {
        QMutexLocker lock(&handlerMutex);
        sink = nullptr;
        qInstallMessageHandler(previousHandler);
    }
}
QStringList DebugConsole::levels() { return levelNames; }
QStringList DebugConsole::facilities() { return facilityNames; }
int DebugConsole::levelOf(const QString &name) {
    const auto lower = name.trimmed().toLower();
    const auto index = levelNames.indexOf(lower);
    if (index >= 0) return int(index);
    if (lower == "error" || lower == "fatal") return lower == "fatal" ? 2 : 3;
    if (lower == "critical") return 2;
    if (lower == "warn") return 4;
    if (lower == "debug" || lower == "trace") return 7;
    return 6;
}
QString DebugConsole::monoFamily() const {
    static const QString family = [] {
#ifndef Q_OS_MACOS
        for (const auto name : {"Cascadia Mono", "JetBrains Mono", "SF Mono", "Consolas", "DejaVu Sans Mono", "Ubuntu Mono", "Liberation Mono"})
            if (QFontDatabase::hasFamily(name)) return QString(name);
#endif
        return QFontDatabase::systemFont(QFontDatabase::FixedFont).family();
    }();
    return family;
}
void DebugConsole::setLogDirectory(const QString &profile) {
    m_profile = profile;
    // A new profile must first be created and validated by fm-engine. On Unix,
    // creating it here with QDir would give it shared default permissions.
    if (!m_backend && QDir(profile).exists()) activateLogDirectory();
    else if (m_backend && m_backend->property("connected").toBool()) activateLogDirectory();
}
void DebugConsole::activateLogDirectory() {
    if (m_profile.isEmpty()) return;
    {
        QMutexLocker lock(&m_mutex);
        if (!m_logDirectory.isEmpty()) return;
    }
    const auto path = QDir(m_profile).filePath("diagnostics");
    if (!QDir().mkpath(path)) return;
    {
        QMutexLocker lock(&m_mutex);
        m_logDirectory = path;
        // The first capture is also persisted if the profile became available later.
        for (const auto &entry : m_entries) if (!entry.console && !entry.reply) writeBounded(path, "gui.log", (fileLine(entry) + '\n').toUtf8());
    }
    emit countsChanged();
}
QString DebugConsole::logDirectory() const { QMutexLocker lock(&m_mutex); return m_logDirectory; }
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
    m_identity = {{"gui", digest(QCoreApplication::applicationFilePath())}, {"engine", digest(engine)}};
}
void DebugConsole::assistanceReply(const QString &, const QString &method, const QVariantMap &result, bool ok) {
    // Kept for the state tab only; never logged.
    if (ok && (method == "assist_settings_get" || method == "assist_settings_save") && result.contains("settings")) m_modelSettings = result.value("settings").toMap();
    if (ok && method == "assist_progress") m_modelProgress = result;
    if (!ok || method != "assist_job") return; // IPC failures are already recorded.
    auto safe = pick(result, {"status", "task"});
    const auto error = pick(result.value("error").toMap(), {"code", "message"});
    const auto job = result.value("job_id").toString();
    auto key = safe; key["error"] = error;
    if (m_jobs.value(job) == key) return;
    if (!m_jobs.contains(job) && m_jobs.size() >= 64) m_jobs.erase(m_jobs.begin());
    m_jobs[job] = key;
    if (!error.value("code").toString().isEmpty()) safe["code"] = error.value("code");
    record(error.isEmpty() ? "info" : "err", "local model", "Job " + safe.value("status").toString() + " " + error.value("message").toString(), safe);
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
    static const QRegularExpression control("[\\x00-\\x1f\\x7f]+");
    text = text.left(8192);
    text.replace(secret, "[secret]"); text.replace(url, "[url]"); text.replace(email, "[email]");
    text.replace(path, "[path]"); text.replace(quoted, "[quoted text]"); text.replace(opaque, "[opaque value]");
    text.replace(control, " ");
    return text.size() > 1024 ? text.left(1024) + " [truncated]" : text;
}
void DebugConsole::record(const QString &level, const QString &source, const QString &message, const QVariantMap &fields) {
    const auto safe = pick(fields, {"method", "request_ref", "phase", "code", "elapsed_ms", "retry_after_secs", "queued", "category", "status", "task", "completed", "total", "failed"});
    DebugLogEntry entry;
    entry.at = QDateTime::currentMSecsSinceEpoch();
    entry.level = levelOf(level);
    entry.facility = facilityFor(source, safe);
    entry.message = scrub(message);
    entry.context = contextOf(safe);
    setJson(entry, fields.value("json"));
    add(std::move(entry), true);
}
void DebugConsole::add(DebugLogEntry entry, bool persist) {
    QMutexLocker lock(&m_mutex);
    entry.sequence = ++m_sequence;
    if (persist) {
        auto line = fileLine(entry);
        if (!entry.json.isEmpty()) line += "  " + entry.json;
        writeBounded(m_logDirectory, "gui.log", (line + '\n').toUtf8());
    }
    const bool typed = entry.console || entry.reply;
    if (!typed && entry.level <= 4) m_recent.push_back({entry.at, entry.level});
    // Paused: the log file keeps recording; the console captures nothing new.
    if (m_paused && !typed) return;
    m_entries.push_back(entry);
    if (m_entries.size() > size_t(retained)) m_entries.pop_front();
    m_pending.push_back(std::move(entry));
    if (!m_flushQueued) {
        // Never touch the model inside a message handler or another model update.
        m_flushQueued = true;
        QMetaObject::invokeMethod(this, [this] { flush(); }, Qt::QueuedConnection);
    }
}
void DebugConsole::say(int level, const QString &message) {
    DebugLogEntry entry;
    entry.at = QDateTime::currentMSecsSinceEpoch();
    entry.level = level;
    entry.facility = "app";
    entry.message = message;
    entry.reply = true;
    add(std::move(entry), false);
}
bool DebugConsole::passes(const DebugLogEntry &e) const {
    if (e.sequence <= m_clearedThrough) return false;
    if (!e.console && !e.reply) {
        if (e.level > m_level) return false;
        if (m_facility != "all" && e.facility != m_facility) return false;
    }
    if (m_grep.isEmpty()) return true;
    return e.message.contains(m_grep, Qt::CaseInsensitive) || e.context.contains(m_grep, Qt::CaseInsensitive)
        || e.json.contains(m_grep, Qt::CaseInsensitive) || e.facility.contains(m_grep, Qt::CaseInsensitive);
}
void DebugConsole::flush() {
    std::deque<DebugLogEntry> pending;
    qulonglong oldest = 0;
    {
        QMutexLocker lock(&m_mutex);
        m_flushQueued = false;
        pending.swap(m_pending);
        oldest = m_entries.empty() ? m_sequence + 1 : m_entries.front().sequence;
    }
    auto &rows = m_model.m_rows;
    std::vector<DebugLogEntry> passing;
    for (auto &entry : pending) if (entry.sequence >= oldest && passes(entry)) passing.push_back(std::move(entry));
    m_appended += passing.size();
    if (passing.size() > 50) {
        // A burst replaces most of the view: one reset keeps the list's geometry exact.
        m_model.beginResetModel();
        for (auto &entry : passing) rows.push_back(std::move(entry));
        while (rows.size() > size_t(shown) || (!rows.empty() && rows.front().sequence < oldest)) rows.pop_front();
        m_model.endResetModel();
        passing.clear();
    }
    for (auto &entry : passing) {
        m_model.beginInsertRows({}, int(rows.size()), int(rows.size()));
        rows.push_back(std::move(entry));
        m_model.endInsertRows();
    }
    // Keep the newest `shown` rows, and none older than the retained history.
    int drop = 0;
    while (drop < int(rows.size()) && (int(rows.size()) - drop > shown || rows[size_t(drop)].sequence < oldest)) ++drop;
    if (drop) {
        m_model.beginRemoveRows({}, 0, drop - 1);
        rows.erase(rows.begin(), rows.begin() + drop);
        m_model.endRemoveRows();
    }
    {
        QMutexLocker lock(&m_mutex);
        int matched = 0;
        for (const auto &entry : m_entries) if (passes(entry)) ++matched;
        m_matched = matched;
    }
    countRecent();
    emit countsChanged();
}
void DebugConsole::rebuild() {
    std::deque<DebugLogEntry> rows;
    int matched = 0;
    {
        QMutexLocker lock(&m_mutex);
        for (const auto &entry : m_entries) if (passes(entry)) {
            ++matched;
            rows.push_back(entry);
            if (rows.size() > size_t(shown)) rows.pop_front();
        }
    }
    m_model.beginResetModel();
    m_model.m_rows.swap(rows);
    m_model.endResetModel();
    m_matched = matched;
    m_appended = 0;
    emit countsChanged();
}
void DebugConsole::countRecent() {
    const auto hour = QDateTime::currentMSecsSinceEpoch() - 3600 * 1000;
    int errors = 0, warnings = 0;
    {
        QMutexLocker lock(&m_mutex);
        while (!m_recent.empty() && m_recent.front().first < hour) m_recent.pop_front();
        for (const auto &[at, level] : m_recent) level <= 3 ? ++errors : ++warnings;
    }
    if (errors == m_errors && warnings == m_warnings) return;
    m_errors = errors; m_warnings = warnings;
    emit countsChanged();
}
void DebugConsole::setLevel(int level) {
    level = qBound(0, level, 7);
    if (level == m_level) return;
    m_level = level; rebuild(); emit filterChanged();
}
void DebugConsole::setFacility(const QString &facility) {
    const auto next = facilityNames.contains(facility) ? facility : QStringLiteral("all");
    if (next == m_facility) return;
    m_facility = next; rebuild(); emit filterChanged();
}
void DebugConsole::setGrep(const QString &grep) {
    if (grep == m_grep) return;
    m_grep = grep; rebuild(); emit filterChanged();
}
void DebugConsole::setPaused(bool paused) {
    {
        QMutexLocker lock(&m_mutex);
        if (paused == m_paused) return;
        m_paused = paused;
    }
    emit pausedChanged();
}
void DebugConsole::clear() {
    {
        QMutexLocker lock(&m_mutex);
        m_clearedThrough = m_sequence;
    }
    rebuild();
}
int DebugConsole::count() const { QMutexLocker lock(&m_mutex); return int(m_entries.size()); }
void DebugConsole::copyText(const QString &text) const { if (auto clipboard = QGuiApplication::clipboard()) clipboard->setText(text); }
QStringList DebugConsole::exportLines() const {
    QStringList lines;
    QMutexLocker lock(&m_mutex);
    for (const auto &entry : m_entries) {
        if (!passes(entry)) continue;
        lines.append(fileLine(entry));
        if (entry.json.isEmpty()) continue;
        const auto pretty = QString::fromUtf8(QJsonDocument::fromJson(entry.json.toUtf8()).toJson(QJsonDocument::Indented)).trimmed();
        for (const auto &line : pretty.split('\n')) lines.append("    " + line);
    }
    return lines;
}
QString DebugConsole::exportLog(const QString &directory) {
    const auto folder = directory.isEmpty() ? QStandardPaths::writableLocation(QStandardPaths::DownloadLocation) : directory;
    if (folder.isEmpty() || !QDir().mkpath(folder)) { say(3, "export failed · no Downloads folder"); return {}; }
    const auto path = QDir(folder).filePath("fuckmail-" + QDateTime::currentDateTime().toString("yyyyMMddHHmm") + ".log");
    QStringList lines;
    int count = 0;
    {
        QMutexLocker lock(&m_mutex);
        for (const auto &entry : m_entries) if (passes(entry)) ++count;
    }
    lines = exportLines();
    QFile file(path);
    if (!file.open(QIODevice::WriteOnly | QIODevice::Truncate)) { say(3, "export failed · " + file.errorString()); return {}; }
    file.write((lines.join('\n') + '\n').toUtf8());
    file.close();
    // The path names the user's home folder, so this notice stays out of the log file.
    say(5, "exported " + QString::number(count) + " lines → " + homeRelative(path));
    return path;
}
QVariantMap DebugConsole::run(const QString &raw) {
    const auto line = raw.trimmed();
    if (line.isEmpty()) return {};
    DebugLogEntry echo;
    echo.at = QDateTime::currentMSecsSinceEpoch();
    echo.level = 5; echo.facility = "console"; echo.console = true; echo.message = "› " + line;
    add(std::move(echo), false);
    static const QRegularExpression space("\\s+");
    const auto parts = line.split(space);
    const auto command = parts.first().toLower();
    const auto argument = parts.mid(1).join(' ');
    const QVariantMap tail{{"tail", true}};
    if (command == "help") {
        for (const auto &text : {QString("help                 this list"), QString("clear                clear the view"),
                 "level <name>         show this level and worse · " + levelNames.join(' '),
                 "fac <name|all>       one facility · " + facilityNames.join(' '),
                 QString("grep <text>          filter lines · grep alone clears"), QString("tail · pause · resume"),
                 QString("state · logs         switch tab"), QString("sync <account>       sync one account now"),
                 QString("export               save filtered lines to ~/Downloads"), QString("exit                 close the console")})
            say(6, text);
        return tail;
    }
    if (command == "clear") { clear(); return tail; }
    if (command == "level") {
        const auto index = levelNames.indexOf(argument.toLower());
        if (index < 0) say(3, "unknown level \"" + argument + "\" · try " + levelNames.join(", "));
        else setLevel(int(index));
        return tail;
    }
    if (command == "fac") {
        const auto name = argument.toLower();
        if (name != "all" && !facilityNames.contains(name)) say(3, "unknown facility \"" + argument + "\" · try all, " + facilityNames.join(", "));
        else setFacility(name);
        return tail;
    }
    if (command == "grep") { setGrep(argument); return tail; }
    if (command == "tail") return tail;
    if (command == "pause" || command == "resume") { setPaused(command == "pause"); return tail; }
    if (command == "state" || command == "logs") return {{"tab", command}, {"tail", true}};
    if (command == "export") { exportLog(); return tail; }
    if (command == "exit" || command == "quit") return {{"close", true}};
    if (command == "sync") {
        if (argument.isEmpty()) { say(3, "usage: sync <account> · an address prefix or display name"); return tail; }
        QVariantMap match;
        for (const auto &item : backendValue("accounts").toList()) {
            const auto row = item.toMap();
            if (row.value("email").toString().startsWith(argument, Qt::CaseInsensitive)
                || row.value("name").toString().compare(argument, Qt::CaseInsensitive) == 0) { match = row; break; }
        }
        if (match.isEmpty() || !m_backend) { say(3, "no account matches \"" + argument + "\""); return tail; }
        const auto id = match.value("id").toString();
        const bool active = backendValue("googleSync").toMap().value(id).toMap().value("active").toBool();
        const bool sent = active ? QMetaObject::invokeMethod(m_backend, "refreshGoogleSync", Q_ARG(QString, id))
                                 : QMetaObject::invokeMethod(m_backend, "startGoogleSync", Q_ARG(QString, id), Q_ARG(QVariantMap, QVariantMap{}));
        const auto address = match.value("email").toString();
        say(sent ? 6 : 3, address + (sent ? ": manual sync requested" : ": sync is unavailable"));
        return tail;
    }
    say(3, command + ": command not found · type help");
    return tail;
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
        const auto scope = event.value("scope").toString();
        if (error.error != QJsonParseError::NoError || !QStringList{"App", "IPC", "Sync", "Auth", "Model", "Calendar", "Mail", "Drafts", "Sending", "Accounts", "Settings", "Search", "Other"}.contains(scope)) continue;
        const auto rawFields = event.value("fields").toObject().toVariantMap();
        const auto fields = pick(rawFields, {"job_ref", "account_ref", "target_ref", "task", "origin", "provider", "model", "model_digest", "prompt_sha256", "label_state", "input_chars", "summary_chars", "sender_kind",
            "category_abstain", "partial", "source_count", "label_choices", "elapsed_ms", "code", "attempt",
            "completed", "total", "service", "state", "connection_generation", "generation", "lane", "items",
            "normalized_bytes", "page_complete", "catalog_refresh", "calendars", "pages", "failures", "more",
            "first_error_code", "pid"});
        DebugLogEntry entry;
        const auto at = QDateTime::fromString(event.value("at").toString(), Qt::ISODateWithMs);
        entry.at = at.isValid() ? at.toMSecsSinceEpoch() : QDateTime::currentMSecsSinceEpoch();
        entry.level = levelOf(event.value("level").toString());
        entry.facility = facilityForScope(scope);
        entry.message = scrub(event.value("event").toString());
        entry.context = contextOf(fields);
        QVariantList categoryIds;
        for (const auto &id : rawFields.value("category_ids").toList().mid(0, 3)) categoryIds.append(scrub(id.toString()).left(80));
        if (!categoryIds.isEmpty()) setJson(entry, QVariantMap{{"category_ids", categoryIds}});
        // The engine writes its own file; this copy is for the console only.
        add(std::move(entry), false);
    }
    if (m_enginePartial.size() > 4096) m_enginePartial.clear();
}
QString DebugConsole::accountAlias(const QString &account) {
    if (account.isEmpty()) return "none";
    if (!m_aliases.contains(account)) m_aliases[account] = "account-" + QString::number(m_aliases.size() + 1);
    return m_aliases.value(account);
}
QVariant DebugConsole::backendValue(const char *name) const { return m_backend ? m_backend->property(name) : QVariant{}; }
// What refresh() compares to log changes, with aliased accounts.
QVariantMap DebugConsole::snapshot() {
    if (!m_backend) return {};
    QVariantMap state;
    QVariantMap ui{{"connected", backendValue("connected")}, {"selected_account", accountAlias(backendValue("account").toString())},
        {"error", scrub(backendValue("error").toString())}};
    state["UI"] = ui;
    state["auth"] = pick(backendValue("googleAuth").toMap(), {"phase", "error_code", "message"});
    QVariantMap sync;
    const auto rawSync = backendValue("googleSync").toMap();
    for (auto it = rawSync.cbegin(); it != rawSync.cend(); ++it) {
        const auto row = it.value().toMap();
        auto safe = pick(row, {"active", "owned", "credential_access_required", "last_mail_sync_at", "refresh_requested"});
        safe["progress"] = pick(row.value("progress").toMap(), {"mail", "mail_error", "mail_steps", "mail_mutations", "mail_mutations_error", "drafts", "drafts_error", "remote_drafts", "remote_drafts_error", "labels", "labels_error", "calendar", "calendar_error", "calendar_pages", "calendar_failures", "calendars", "calendar_catalog", "calendar_events", "calendar_free_busy"});
        sync[accountAlias(it.key())] = safe;
    }
    state["sync"] = sync;
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
                    record(p.key().endsWith("_error") && !p.value().toString().isEmpty() ? "warning" : "info", "sync",
                        it.key() + " " + p.key() + " = " + p.value().toString());
            } else record("info", "sync", it.key() + " " + field.key() + " = " + field.value().toString());
        }
    }
    for (auto it = m_lastSync.cbegin(); it != m_lastSync.cend(); ++it) if (!sync.contains(it.key())) record("info", "sync", it.key() + " no longer in sync snapshot");
    m_lastSync = sync;
    const auto ui = next.value("UI").toMap(), before = m_state.value("UI").toMap();
    for (const auto key : {"connected", "selected_account", "error"}) if (ui.value(key) != before.value(key))
        record(QString(key) == "error" && !ui.value(key).toString().isEmpty() ? "err" : "info", "app", QString(key) + " = " + ui.value(key).toString());
    if (next.value("auth") != m_state.value("auth")) {
        const auto auth = next.value("auth").toMap();
        record(auth.value("error_code").toString().isEmpty() ? "info" : "warning", "auth", auth.value("phase").toString() + " " + auth.value("message").toString(), {{"code", auth.value("error_code")}});
    }
    m_state = next;
    countRecent();
}
void DebugConsole::requestState() {
    if (!m_backend || !m_backend->property("connected").toBool()) return;
    for (const auto method : {"assist_settings_get", "assist_progress"}) {
        QString ticket;
        QMetaObject::invokeMethod(m_backend, "assistanceRequest", Q_RETURN_ARG(QString, ticket),
            Q_ARG(QString, QString(method)), Q_ARG(QVariantMap, QVariantMap{}), Q_ARG(QString, QString{}));
    }
    if (auto html = backendValue("htmlContent").value<QObject *>()) QMetaObject::invokeMethod(html, "refreshImageCache");
}
QVariantList DebugConsole::stateRows(const QVariantMap &ui) {
    QVariantList rows;
    const auto section = [&](int depth, const QString &key) { rows.append(QVariantMap{{"depth", depth}, {"key", key}, {"value", ""}, {"kind", "section"}}); };
    const auto value = [&](int depth, const QString &key, const QVariant &v) {
        QString kind = "string", text = v.toString();
        const auto type = v.metaType().id();
        if (!v.isValid() || v.isNull() || (type == QMetaType::QString && text.isEmpty())) { kind = "null"; text = "null"; }
        else if (type == QMetaType::Bool) { kind = "bool"; text = v.toBool() ? "true" : "false"; }
        else if (type == QMetaType::Int || type == QMetaType::LongLong || type == QMetaType::UInt || type == QMetaType::ULongLong || type == QMetaType::Double) kind = "number";
        else text = '"' + text + '"';
        rows.append(QVariantMap{{"depth", depth}, {"key", key}, {"value", text}, {"kind", kind}});
    };
    const auto minutes = m_uptime.elapsed() / 60000;
    const auto build = m_identity.value("gui").toString();
    section(0, "app");
    value(1, "version", QCoreApplication::applicationVersion() + (build.size() >= 10 ? " (build " + build.left(10) + ")" : QString{}));
    value(1, "qt", QString::fromLatin1(qVersion()));
    value(1, "pid", QCoreApplication::applicationPid());
    value(1, "uptime", minutes < 60 ? QString::number(minutes) + " min" : QString::number(minutes / 60) + " h " + QString::number(minutes % 60) + " min");
    value(1, "theme", ui.value("theme"));
    value(1, "log_lines", count());
    section(0, "ui");
    value(1, "view", ui.value("view"));
    const auto current = backendValue("currentAccount").toMap();
    value(1, "account", current.value("name").toString().isEmpty() ? current.value("email") : current.value("name"));
    value(1, "thread", backendValue("selectedId"));
    value(1, "settings_tab", ui.value("settings_tab"));
    section(0, "accounts");
    const auto sync = backendValue("googleSync").toMap();
    const auto choices = m_modelSettings.value("accounts").toMap();
    for (const auto &item : backendValue("accounts").toList()) {
        const auto row = item.toMap();
        const auto id = row.value("id").toString();
        const auto state = sync.value(id).toMap();
        const auto progress = state.value("progress").toMap();
        const auto mail = progress.value("mail").toString();
        QString status = !state.value("active").toBool() ? "paused" : "idle";
        if (state.value("credential_access_required").toBool()) status = "reconnect required";
        else if (!progress.value("mail_error").toString().isEmpty()) status = "error · " + progress.value("mail_error").toString();
        else if (state.value("active").toBool() && (mail == "starting" || mail == "running" || mail == "retrying")) status = "syncing";
        auto last = state.value("last_mail_sync_at");
        if (last.metaType().id() == QMetaType::QString) { const auto t = QDateTime::fromString(last.toString(), Qt::ISODateWithMs); if (t.isValid()) last = t.toLocalTime().toString("yyyy-MM-dd HH:mm:ss"); }
        else if (last.canConvert<qlonglong>() && last.toLongLong() > 0) last = QDateTime::fromSecsSinceEpoch(last.toLongLong()).toString("yyyy-MM-dd HH:mm:ss");
        const auto choice = choices.value(id).toMap();
        section(1, row.value("name").toString().isEmpty() ? row.value("email").toString() : row.value("name").toString());
        value(2, "address", row.value("email"));
        value(2, "status", status);
        value(2, "last_sync", last);
        value(2, "retention", row.value("mail_retention").toString().isEmpty() ? QString("all") : row.value("mail_retention").toString());
        value(2, "auto_summarize", choice.contains("summaries") ? choice.value("summaries").toBool() : true);
        value(2, "auto_label", choice.contains("classification") ? choice.value("classification").toBool() : true);
    }
    section(0, "llm");
    const bool known = !m_modelSettings.isEmpty();
    int queued = 0; double seconds = 0; int timed = 0; QString modelError; bool configured = false;
    for (const auto &item : m_modelProgress.value("accounts").toList()) {
        const auto row = item.toMap();
        queued += row.value("queued").toInt();
        configured = configured || row.value("model_configured").toBool();
        if (row.value("seconds_per_message").toDouble() > 0) { seconds += row.value("seconds_per_message").toDouble(); ++timed; }
        if (modelError.isEmpty() && row.value("model_error").isValid() && !row.value("model_error").isNull())
            modelError = row.value("model_error").toMap().value("message").toString();
    }
    const auto model = m_modelSettings.value("model");
    value(1, "provider", known ? m_modelSettings.value("provider", "ollama") : QVariant{});
    value(1, "endpoint", m_modelSettings.value("endpoint"));
    value(1, "status", !known ? QString("unknown") : !modelError.isEmpty() ? "error · " + modelError
        : (configured || model.toMap().contains("name")) ? QString("ok") : QString("not set up"));
    value(1, "model", model.toMap().value("name"));
    value(1, "queue", m_modelProgress.isEmpty() ? QVariant{} : QVariant(queued));
    value(1, "avg_latency", timed ? QVariant(QString::number(seconds / timed, 'f', 2) + " s") : QVariant{});
    section(0, "network");
    const auto html = backendValue("htmlContent").value<QObject *>();
    value(1, "image_proxy", html ? QVariant(html->property("proxy").toMap().value("enabled").toBool()) : QVariant{});
    value(1, "blocked_domains", html ? QVariant(int(html->property("blockedHosts").toList().size())) : QVariant{});
    value(1, "image_cache", html && html->property("imageCache").toMap().contains("bytes") ? QVariant(bytesText(html->property("imageCache").toMap().value("bytes").toLongLong())) : QVariant{});
    section(0, "db");
    const auto profile = m_profile.isEmpty() ? backendValue("profilePath").toString() : m_profile;
    const auto store = profile.isEmpty() ? QString{} : QDir(profile).filePath("store.sqlite3");
    const QFileInfo storeInfo(store), walInfo(store + "-wal");
    value(1, "path", store.isEmpty() ? QVariant{} : QVariant(homeRelative(store)));
    value(1, "size", storeInfo.exists() ? QVariant(bytesText(storeInfo.size())) : QVariant{});
    value(1, "schema", backendValue("diagnostics").toMap().value("schema_version"));
    value(1, "wal_pages", storeInfo.exists() ? QVariant(walInfo.exists() ? int(walInfo.size() / 4096) : 0) : QVariant{});
    return rows;
}
