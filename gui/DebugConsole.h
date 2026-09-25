#pragma once
#include <QAbstractListModel>
#include <QObject>
#include <QPointer>
#include <QTimer>
#include <QElapsedTimer>
#include <QDateTime>
#include <QMutex>
#include <QVariantMap>
#include <deque>
#include <vector>

// One retained console line. Levels are syslog 0 (emerg) to 7 (debug).
struct DebugLogEntry {
    qulonglong sequence = 0;
    qint64 at = 0; // UTC milliseconds since the epoch
    int level = 6;
    QString facility, message, context, json; // json: compact, "" when none
    int jsonLines = 0;
    // A typed command's echo, and the console's reply to it: shown whatever the
    // level and facility filters, never written to the log file.
    bool console = false, reply = false;
};

// The console's view: the latest 600 retained lines that pass the filters.
class DebugLogModel final : public QAbstractListModel {
    Q_OBJECT
public:
    enum Role { SequenceRole = Qt::UserRole + 1, TimeRole, LevelRole, FacilityRole, MessageRole, ContextRole,
        JsonRole, JsonLinesRole, ConsoleRole };
    using QAbstractListModel::QAbstractListModel;
    int rowCount(const QModelIndex &parent = {}) const override { return parent.isValid() ? 0 : int(m_rows.size()); }
    QVariant data(const QModelIndex &index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;
private:
    friend class DebugConsole;
    std::deque<DebugLogEntry> m_rows;
};

// Metadata-only diagnostics. Never serialize request/response bodies or credentials.
class DebugConsole final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QObject *lines READ lines CONSTANT)
    Q_PROPERTY(QStringList levels READ levels CONSTANT)
    Q_PROPERTY(QStringList facilities READ facilities CONSTANT)
    Q_PROPERTY(QString monoFamily READ monoFamily CONSTANT)
    Q_PROPERTY(int level READ level WRITE setLevel NOTIFY filterChanged)
    Q_PROPERTY(QString facility READ facility WRITE setFacility NOTIFY filterChanged)
    Q_PROPERTY(QString grep READ grep WRITE setGrep NOTIFY filterChanged)
    Q_PROPERTY(bool paused READ paused WRITE setPaused NOTIFY pausedChanged)
    // Retained lines, lines passing the filters, and passing lines added since the last filter change.
    Q_PROPERTY(int count READ count NOTIFY countsChanged)
    Q_PROPERTY(int matched READ matched NOTIFY countsChanged)
    Q_PROPERTY(qulonglong appended READ appended NOTIFY countsChanged)
    Q_PROPERTY(int errors READ errors NOTIFY countsChanged)
    Q_PROPERTY(int warnings READ warnings NOTIFY countsChanged)
    Q_PROPERTY(QString logDirectory READ logDirectory NOTIFY countsChanged)
public:
    static constexpr int retained = 5000, shown = 600;
    explicit DebugConsole(QObject *parent = nullptr);
    ~DebugConsole() override;
    void attach(QObject *backend);
    void captureQtMessages();
    void identifyRuntime(const QString &engine);
    void setLogDirectory(const QString &profile);
    QObject *lines() { return &m_model; }
    static QStringList levels();
    static QStringList facilities();
    QString monoFamily() const;
    int level() const { return m_level; }
    void setLevel(int level);
    QString facility() const { return m_facility; }
    void setFacility(const QString &facility);
    QString grep() const { return m_grep; }
    void setGrep(const QString &grep);
    bool paused() const { return m_paused; }
    void setPaused(bool paused);
    int count() const;
    int matched() const { return m_matched; }
    qulonglong appended() const { return m_appended; }
    int errors() const { return m_errors; }
    int warnings() const { return m_warnings; }
    QString logDirectory() const;
    // Filtered lines in the export format, oldest first.
    Q_INVOKABLE QStringList exportLines() const;
    // Writes exportLines() to the Downloads folder; returns the path, or "".
    Q_INVOKABLE QString exportLog(const QString &directory = {});
    // Runs one command line. Returns what the view must do: {tab}, {close}, {tail} or {}.
    Q_INVOKABLE QVariantMap run(const QString &command);
    // Hides every line retained so far; the log file keeps them.
    Q_INVOKABLE void clear();
    Q_INVOKABLE void copyText(const QString &text) const;
    // The state tab: [{depth, key, value, kind}] with kind section|string|number|bool|null.
    Q_INVOKABLE QVariantList stateRows(const QVariantMap &ui = {});
    // Asks the engine for the model settings and queue shown in the state tab.
    Q_INVOKABLE void requestState();
    static QString scrub(QString text);
    static int levelOf(const QString &name);
public slots:
    void record(const QString &level, const QString &source, const QString &message, const QVariantMap &fields = {});
    void refresh();
    void assistanceReply(const QString &ticket, const QString &method, const QVariantMap &result, bool ok);
signals:
    void filterChanged();
    void pausedChanged();
    void countsChanged();
private:
    bool passes(const DebugLogEntry &entry) const;
    void add(DebugLogEntry entry, bool persist);
    void say(int level, const QString &message);
    void flush();
    void rebuild();
    void countRecent();
    void activateLogDirectory();
    void readEngineLog();
    QString accountAlias(const QString &account);
    QVariantMap snapshot();
    QVariant backendValue(const char *name) const;

    DebugLogModel m_model;
    QPointer<QObject> m_backend;
    QTimer m_timer;
    QElapsedTimer m_uptime;
    QVariantMap m_state, m_lastSync, m_identity;
    QVariantMap m_modelSettings, m_modelProgress;
    QMap<QString, QVariantMap> m_jobs;
    QMap<QString, QString> m_aliases;
    // Guarded by m_mutex: written from any thread through the Qt message handler.
    mutable QMutex m_mutex;
    std::deque<DebugLogEntry> m_entries, m_pending;
    std::deque<std::pair<qint64, int>> m_recent; // (at, level) of err-and-worse and warnings
    qulonglong m_sequence = 0;
    bool m_flushQueued = false;
    QString m_logDirectory;
    // Main thread only.
    int m_level = 6;
    QString m_facility = "all", m_grep;
    bool m_paused = false;
    qulonglong m_clearedThrough = 0;
    int m_matched = 0, m_errors = 0, m_warnings = 0;
    qulonglong m_appended = 0;
    QString m_profile;
    qint64 m_engineOffset = 0;
    QDateTime m_engineBirthTime;
    QByteArray m_enginePartial;
    bool m_capturing = false;
};
