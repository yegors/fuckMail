#pragma once
#include <QObject>
#include <QPointer>
#include <QTimer>
#include <QElapsedTimer>
#include <QMutex>
#include <QVariantMap>
#include <deque>

// Session-only diagnostics. Never serialize request/response bodies or a profile.
class DebugConsole final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QString stateText READ stateText NOTIFY changed)
    Q_PROPERTY(int count READ count NOTIFY changed)
    Q_PROPERTY(qulonglong dropped READ dropped NOTIFY changed)
public:
    explicit DebugConsole(QObject *parent = nullptr);
    ~DebugConsole() override;
    void attach(QObject *backend);
    void captureQtMessages();
    void identifyRuntime(const QString &engine);
    QString stateText() const;
    int count() const;
    qulonglong dropped() const;
    Q_INVOKABLE QString events(const QString &level = {}, const QString &filter = {}) const;
    Q_INVOKABLE QString report() const;
    Q_INVOKABLE void copyReport() const;
    Q_INVOKABLE void clear();
    static QString scrub(QString text);
public slots:
    void record(const QString &level, const QString &source, const QString &message, const QVariantMap &fields = {});
    void refresh();
    void assistanceReply(const QString &ticket, const QString &method, const QVariantMap &result, bool ok);
signals:
    void changed();
private:
    QString accountAlias(const QString &account);
    QVariantMap snapshot();
    QPointer<QObject> m_backend;
    QTimer m_timer;
    QElapsedTimer m_uptime;
    QString m_session;
    QVariantMap m_state, m_lastSync;
    QVariantMap m_identity, m_assistance;
    QMap<QString, QVariantMap> m_jobs;
    QMap<QString, QString> m_aliases;
    struct Entry { QString level, line; };
    mutable QMutex m_mutex;
    std::deque<Entry> m_entries;
    qulonglong m_dropped = 0;
    bool m_dirty = false, m_capturing = false;
};
