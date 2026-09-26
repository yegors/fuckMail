// A single engine request on a dedicated IPC connection, used for long-running
// work (Google operations, attachment imports, the HTML reader's calls) so it
// never waits behind, or blocks, MailBackend's main request queue.
#pragma once
#include <QObject>
#include <QJsonObject>
#include <QLocalSocket>
#include <QTimer>
#include <functional>

/// One explicit provider operation or local draft attachment import. Its
/// connection also supports one cancellable foreground conversation read.
/// Closing the connection cancels its engine owner; durable outcomes remain there.
///
/// Single use and GUI thread only. The owner (MailBackend's split backends or
/// HtmlContent) creates one per request as a QObject child, keeps a pointer so
/// it can cancel it, and calls deleteLater() from the callback.
class GoogleOperation final : public QObject {
public:
    /// Receives the engine's `result` object with `ok` true, or an error
    /// object `{code, message}` with `ok` false. Local failures use codes
    /// such as `timeout`, `cancelled`, `engine_disconnected`,
    /// `engine_untrusted`, `invalid_request`, `invalid_response` and
    /// `profile_mode_mismatch`. The callback may delete the operation.
    using Callback = std::function<void(const QJsonObject &, bool)>;
    /// `account` goes into every envelope after hello; empty means an
    /// account-free call. `requestId` is a caller label for finding the
    /// operation later; it is not sent.
    GoogleOperation(QString account, QString requestId, QObject *parent);
    /// Cancels without calling the callback.
    ~GoogleOperation() override;
    /// Connects to the engine recorded in `profile`'s private discovery file
    /// and runs `method` with `params`. Only an allow-listed set of methods
    /// is accepted.
    ///
    /// The discovery record must still name `enginePid`, and the connected
    /// peer must be that process under the same OS user before the session
    /// token is sent. Non-live profiles are refused except for local reads
    /// and imports. The callback runs exactly once, unless cancelled with
    /// `notify` false; it can run synchronously inside start() when the method
    /// or discovery is rejected. Later calls to start() are ignored.
    ///
    /// Each phase has a timeout: 15 s to connect and complete hello, then a
    /// per-method limit from 15 s for reader calls to 10 minutes for provider
    /// writes. A timeout never retries the request.
    void start(const QString &profile, qint64 enginePid, const QString &method,
               const QJsonObject &params, Callback callback);
    /// Closes the connection, which cancels the engine's work for it; any
    /// provider outcome that already happened stays recorded by the engine.
    /// With `notify`, the callback receives a `cancelled` error
    /// synchronously. Does nothing once finished.
    void cancel(bool notify = true);
    QString account() const { return m_account; }
    QString requestId() const { return m_requestId; }
    QString method() const { return m_method; }
private:
    friend class GuiCompositionSelfTest;
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
