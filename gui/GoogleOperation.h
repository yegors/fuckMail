#pragma once
#include <QObject>
#include <QJsonObject>
#include <QLocalSocket>
#include <QTimer>
#include <functional>

// One explicit provider operation. Its connection never carries cached queries.
// Closing the connection cancels its engine owner; durable outcomes remain there.
class GoogleOperation final : public QObject {
public:
    using Callback = std::function<void(const QJsonObject &, bool)>;
    GoogleOperation(QString account, QString requestId, QObject *parent);
    ~GoogleOperation() override;
    void start(const QString &profile, qint64 enginePid, const QString &method,
               const QJsonObject &params, Callback callback);
    void cancel(bool notify = true);
    QString account() const { return m_account; }
    QString requestId() const { return m_requestId; }
private:
    void receive();
    void finish(const QJsonObject &result, bool ok);
    void reject(const QString &code, const QString &message);
    void write(const QString &id, const QString &method, const QJsonObject &params);
    QLocalSocket m_socket;
    QTimer m_timer;
    QByteArray m_buffer;
    QJsonObject m_params;
    QString m_account, m_requestId, m_method, m_token;
    qint64 m_enginePid = 0;
    Callback m_callback;
    bool m_hello = false, m_finished = false, m_started = false;
};
