#pragma once
#include "GoogleOperation.h"
#include "IpcTrust.h"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QJsonDocument>
#include <QLocalServer>
#include <QPointer>
#include <QTemporaryDir>
#include <QUuid>
#include <QVariant>
#include <cstdlib>
#include <memory>
#ifdef Q_OS_WIN
#define NOMINMAX
#include <windows.h>
#include <sddl.h>
#endif

namespace GoogleOperationFixture {
void require(bool condition, const char *message) {
    if (!condition) { qCritical() << message; std::exit(1); }
}
template<class Predicate> void await(Predicate predicate, const char *message) {
    QElapsedTimer deadline; deadline.start();
    while (!predicate() && deadline.elapsed() < 3000)
        QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    require(predicate(), message);
}
QByteArray reply(const QString &id, const QJsonObject &result) {
    return QJsonDocument(QJsonObject{{"version", 1}, {"id", id}, {"ok", true}, {"result", result}})
        .toJson(QJsonDocument::Compact) + '\n';
}
#ifdef Q_OS_WIN
struct PrivateSecurity {
    PSECURITY_DESCRIPTOR value = nullptr;
    QString sid;
    ~PrivateSecurity() { if (value) LocalFree(value); }
    bool initialize() {
        HANDLE token = nullptr;
        if (!OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &token)) return false;
        DWORD size = 0;
        GetTokenInformation(token, TokenUser, nullptr, 0, &size);
        QByteArray bytes(size, '\0');
        const bool loaded = GetTokenInformation(token, TokenUser, bytes.data(), size, &size);
        CloseHandle(token);
        if (!loaded) return false;
        LPWSTR raw = nullptr;
        if (!ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER *>(bytes.data())->User.Sid, &raw)) return false;
        sid = QString::fromWCharArray(raw);
        LocalFree(raw);
        const auto descriptor = "O:" + sid + "D:P(A;OICI;GA;;;" + sid + ")";
        return ConvertStringSecurityDescriptorToSecurityDescriptorW(
            reinterpret_cast<LPCWSTR>(descriptor.utf16()), SDDL_REVISION_1, &value, nullptr);
    }
    SECURITY_ATTRIBUTES attributes() { return {sizeof(SECURITY_ATTRIBUTES), value, FALSE}; }
};
#endif

enum class Behavior { Hold, Malformed, OutOfOrder, Oversized, WrongMode, WrongType, Duplicate };
struct Engine {
#ifdef Q_OS_WIN
    QTemporaryDir temporary;
    PrivateSecurity security;
#else
    QTemporaryDir temporary{"/tmp/fm-operation-XXXXXX"};
#endif
    QString profile;
    QLocalServer server;
    QPointer<QLocalSocket> operationPeer;
    Behavior behavior;
    QString expectedMethod = "google_calendar_write", profileMode = "live";
    QJsonObject expectedParams, operationParams;
    std::function<bool(QLocalSocket *, const QJsonObject &)> customRequest;
    int connections = 0, hellos = 0, operations = 0, queries = 0, operationCloses = 0;
    explicit Engine(Behavior mode = Behavior::Hold) : behavior(mode) {
        require(temporary.isValid(), "temporary operation fixture");
        profile = QDir(temporary.path()).filePath("profile");
#ifdef Q_OS_WIN
        require(security.initialize(), "private Windows descriptor");
        auto attributes = security.attributes();
        require(CreateDirectoryW(reinterpret_cast<LPCWSTR>(profile.utf16()), &attributes), "private Windows profile");
        server.setSocketOptions(QLocalServer::UserAccessOption);
        require(server.listen("fm-operation-test-" + QUuid::createUuid().toString(QUuid::Id128)), "named-pipe fixture");
#else
        require(QDir().mkpath(profile), "private Unix profile");
        require(QFile::setPermissions(profile, QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner), "profile mode 0700");
        require(server.listen(QDir(profile).filePath("peer.sock")), "local socket fixture");
#endif
        discovery(QCoreApplication::applicationPid());
        QString error;
        require(!readPrivateDiscovery(profile, error).isEmpty() && error.isEmpty(), "actual private discovery accepted");
        QObject::connect(&server, &QLocalServer::newConnection, &server, [this] {
            while (auto *peer = server.nextPendingConnection()) {
                ++connections;
                QObject::connect(peer, &QLocalSocket::readyRead, peer, [this, peer] { receive(peer); });
                QObject::connect(peer, &QLocalSocket::disconnected, peer, [this, peer] {
                    if (peer->property("operation").toBool()) ++operationCloses;
                });
                receive(peer);
            }
        });
    }
    void discovery(qint64 pid) {
        const auto path = QDir(profile).filePath("engine.json");
        if (QFile::exists(path)) require(QFile::remove(path), "replace only fixture discovery");
        const auto bytes = QJsonDocument(QJsonObject{{"protocol", 1}, {"pid", pid},
            {"endpoint", server.fullServerName()}, {"token", "synthetic-operation-token"}}).toJson();
#ifdef Q_OS_WIN
        auto attributes = security.attributes();
        auto file = CreateFileW(reinterpret_cast<LPCWSTR>(path.utf16()), GENERIC_WRITE,
            FILE_SHARE_READ, &attributes, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, nullptr);
        require(file != INVALID_HANDLE_VALUE, "private Windows discovery creation");
        DWORD written = 0;
        const bool ok = WriteFile(file, bytes.constData(), DWORD(bytes.size()), &written, nullptr);
        CloseHandle(file);
        require(ok && written == DWORD(bytes.size()), "private Windows discovery content");
#else
        QFile file(path);
        require(file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size(), "private Unix discovery content");
        file.close();
        require(file.setPermissions(QFile::ReadOwner | QFile::WriteOwner), "discovery mode 0600");
#endif
    }
    void exposeDiscovery() {
        const auto path = QDir(profile).filePath("engine.json");
#ifdef Q_OS_WIN
        PSECURITY_DESCRIPTOR shared = nullptr;
        const auto descriptor = "D:P(A;;GA;;;" + security.sid + ")(A;;GR;;;WD)";
        require(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            reinterpret_cast<LPCWSTR>(descriptor.utf16()), SDDL_REVISION_1, &shared, nullptr), "shared fixture descriptor");
        const bool changed = SetFileSecurityW(reinterpret_cast<LPCWSTR>(path.utf16()),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, shared);
        LocalFree(shared);
        require(changed, "shared discovery fixture ACL");
#else
        require(QFile::setPermissions(path, QFile::ReadOwner | QFile::WriteOwner | QFile::ReadOther), "shared discovery fixture mode");
#endif
    }
    void receive(QLocalSocket *peer) {
        auto buffer = peer->property("buffer").toByteArray() + peer->readAll();
        while (buffer.contains('\n')) {
            const auto end = buffer.indexOf('\n');
            const auto request = QJsonDocument::fromJson(buffer.left(end)).object();
            buffer.remove(0, end + 1);
            require(request["version"].toInt() == 1, "framed request version");
            const auto method = request["method"].toString();
            const auto id = request["id"].toString();
            if (method == "hello") {
                require(!peer->property("authenticated").toBool(), "one hello per connection");
                require(request["params"].toObject()["token"] == "synthetic-operation-token", "private handshake token");
                peer->setProperty("authenticated", true); ++hellos;
                peer->write(reply(behavior == Behavior::OutOfOrder ? "operation" : id,
                    {{"mode", behavior == Behavior::WrongMode ? "synthetic" : profileMode}}));
            } else {
                require(peer->property("authenticated").toBool(), "authenticate before query or operation");
                if (method == "status") {
                    ++queries;
                    peer->write(reply(id, {{"cached", true}}));
                } else if (customRequest && customRequest(peer, request)) {
                } else {
                    require(method == expectedMethod, "exact allowed operation method");
                    require(request["account"] == "fixture-account", "operation account identity");
                    operationParams = request["params"].toObject();
                    if (expectedParams.isEmpty()) require(operationParams["request_id"] == "fixture-request", "unchanged durable request identity");
                    else require(operationParams == expectedParams, "exact operation parameters without added fields");
                    ++operations; operationPeer = peer; peer->setProperty("operation", true);
                    if (behavior == Behavior::Malformed) peer->write("not-json\n");
                    else if (behavior == Behavior::Oversized) peer->write(QByteArray(1024 * 1024 + 1, 'x'));
                    else if (behavior == Behavior::WrongType) peer->write("{\"version\":1,\"id\":\"operation\",\"ok\":\"true\",\"result\":{}}\n");
                    else if (behavior == Behavior::Duplicate) peer->write(reply(id, {}) + reply(id, {}));
                }
            }
        }
        peer->setProperty("buffer", buffer);
    }
    void complete(const QJsonObject &result = {{"state", "acknowledged"}, {"request_id", "fixture-request"}}) {
        require(operationPeer && operationPeer->state() == QLocalSocket::ConnectedState, "pending operation owner exists");
        operationPeer->write(reply("operation", result));
    }
};
}
