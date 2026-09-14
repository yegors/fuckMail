#include "GoogleOperation.h"
#include "IpcTrust.h"
#include <QJsonDocument>
#include <utility>

namespace { constexpr qsizetype maxFrame = 1024 * 1024; }

GoogleOperation::GoogleOperation(QString account, QString requestId, QObject *parent)
    : QObject(parent), m_account(std::move(account)), m_requestId(std::move(requestId)) {
    m_socket.setReadBufferSize(maxFrame + 1);
    m_timer.setSingleShot(true);
    connect(&m_timer, &QTimer::timeout, this, [this] {
        reject("timeout", m_method == "draft_attachment_import"
            ? "Attachment import timed out. Retry the same attachment to check its saved result."
            : "Google work timed out. Check its saved activity before trying again.");
    });
    connect(&m_socket, &QLocalSocket::connected, this, [this] {
        QString error;
        if (!verifyEnginePeer(m_socket.socketDescriptor(), m_enginePid, error)) {
            reject("engine_untrusted", error); return;
        }
        const auto token = std::exchange(m_token, QString{});
        write("hello", "hello", {{"token", token}, {"client", "gui-operation"}});
    });
    connect(&m_socket, &QLocalSocket::readyRead, this, &GoogleOperation::receive);
    connect(&m_socket, &QLocalSocket::disconnected, this, [this] {
        reject("engine_disconnected", "The local engine disconnected. Check saved activity after reconnecting.");
    });
    connect(&m_socket, &QLocalSocket::errorOccurred, this, [this](auto) {
        reject("engine_disconnected", "The local engine connection failed. Check saved activity after reconnecting.");
    });
}

GoogleOperation::~GoogleOperation() { cancel(false); }

void GoogleOperation::start(const QString &profile, qint64 enginePid, const QString &method,
                            const QJsonObject &params, Callback callback) {
    if (m_started || m_finished) return;
    m_started = true; m_method = method; m_callback = std::move(callback);
    if (method != "google_calendar_write" && method != "google_calendar_reconcile" && method != "google_calendar_occurrence_read"
        && method != "google_calendar_resume" && method != "google_labels_refresh"
        && method != "google_draft_reconcile" && method != "google_draft_resolve"
        && method != "google_send" && method != "google_send_reconcile" && method != "google_send_abandon"
        && method != "google_draft_authoring" && method != "google_draft_copy" && method != "google_draft_adopt"
        && method != "google_draft_delete" && method != "google_draft_delete_reconcile" && method != "google_draft_delete_abandon"
        && method != "draft_attachment_import" && method != "google_draft_source" && method != "google_drafts_refresh") {
        reject("invalid_request", "Unsupported Google operation."); return;
    }
    QString error;
    const auto info = readPrivateDiscovery(profile, error);
    if (!error.isEmpty()) { reject("engine_untrusted", error); return; }
    auto endpoint = info["endpoint"].toString();
    if (info["protocol"].toInt() != 1 || info["pid"].toInteger() != enginePid || enginePid <= 0
        || endpoint.isEmpty() || info["token"].toString().isEmpty()) {
        reject("engine_disconnected", "The local engine changed. Reconnect before continuing."); return;
    }
    m_enginePid = enginePid; m_method = method; m_params = params; m_token = info["token"].toString();
#ifdef Q_OS_WIN
    if (endpoint.startsWith("\\\\.\\pipe\\")) endpoint = endpoint.mid(9);
#endif
    m_timer.start(15000);
    m_socket.connectToServer(endpoint);
}

void GoogleOperation::write(const QString &id, const QString &method, const QJsonObject &params) {
    QJsonObject envelope{{"version", 1}, {"id", id}, {"method", method}, {"params", params}};
    if (method != "hello") envelope["account"] = m_account;
    const auto wire = QJsonDocument(envelope).toJson(QJsonDocument::Compact) + '\n';
    if (wire.size() > maxFrame) { reject("invalid_request", "This Google request exceeds the local interface limit."); return; }
    if (m_socket.write(wire) != wire.size()) reject("engine_disconnected", "The local engine could not receive this request.");
}

void GoogleOperation::receive() {
    if (m_finished) return;
    m_buffer += m_socket.readAll();
    const auto end = m_buffer.indexOf('\n');
    if (m_buffer.size() > maxFrame || (end >= 0 && end + 1 != m_buffer.size())) {
        reject("invalid_response", "The local engine returned an oversized or unexpected response."); return;
    }
    if (end < 0) return;
    QJsonParseError parse;
    const auto document = QJsonDocument::fromJson(m_buffer.left(end), &parse);
    m_buffer.clear();
    const auto response = document.object();
    const bool ok = response["ok"].toBool();
    if (parse.error != QJsonParseError::NoError || !document.isObject()
        || response["version"].toInt() != 1 || response["id"].toString() != (m_hello ? "operation" : "hello")
        || !response["ok"].isBool() || !(ok ? response["result"].isObject() : response["error"].isObject())) {
        reject("invalid_response", "The local engine returned an incompatible response."); return;
    }
    const auto result = response[ok ? "result" : "error"].toObject();
    if (!ok || m_hello) { finish(result, ok); return; }
    const auto mode = result["mode"].toString();
    const bool localImport = m_method == "draft_attachment_import" || m_method == "google_draft_source"
        || m_method == "google_draft_authoring" || m_method == "google_draft_copy";
    if (mode != "live" && !(localImport && mode == "synthetic")) {
        reject("profile_mode_mismatch", "Google operations require a live profile."); return;
    }
    m_hello = true;
    // Quota cooldown can exceed the ordinary local query timeout. The user can
    // cancel at any time; no timeout automatically repeats a provider mutation.
    m_timer.start(localImport ? 60 * 1000 : 10 * 60 * 1000);
    const auto params = std::exchange(m_params, QJsonObject{});
    write("operation", m_method, params);
}

void GoogleOperation::finish(const QJsonObject &result, bool ok) {
    if (m_finished) return;
    m_finished = true; m_timer.stop(); m_token.clear(); m_params = {}; m_buffer.clear();
    QObject::disconnect(&m_socket, nullptr, this, nullptr);
    m_socket.abort();
    auto callback = std::move(m_callback); m_callback = {};
    if (callback) callback(result, ok);
}
void GoogleOperation::reject(const QString &code, const QString &message) {
    finish({{"code", code}, {"message", message}}, false);
}
void GoogleOperation::cancel(bool notify) {
    if (!notify) m_callback = {};
    reject("cancelled", m_method == "draft_attachment_import"
        ? "Attachment import stopped. Retry the same attachment to check its saved result."
        : "Google work cancelled. Its saved activity records any provider outcome.");
}
