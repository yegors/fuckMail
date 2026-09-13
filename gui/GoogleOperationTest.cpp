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

namespace {
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
                    {{"mode", behavior == Behavior::WrongMode ? "synthetic" : "live"}}));
            } else {
                require(peer->property("authenticated").toBool(), "authenticate before query or operation");
                if (method == "status") {
                    ++queries;
                    peer->write(reply(id, {{"cached", true}}));
                } else {
                    require(method == "google_calendar_write", "exact allowed operation method");
                    require(request["account"] == "fixture-account", "operation account identity");
                    require(request["params"].toObject()["request_id"] == "fixture-request", "unchanged durable request identity");
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
    void complete() {
        require(operationPeer && operationPeer->state() == QLocalSocket::ConnectedState, "pending operation owner exists");
        operationPeer->write(reply("operation", {{"state", "acknowledged"}, {"request_id", "fixture-request"}}));
    }
};
struct Completion {
    int calls = 0;
    bool ok = false;
    QJsonObject value;
    GoogleOperation::Callback callback() {
        return [this](const QJsonObject &result, bool success) { ++calls; ok = success; value = result; };
    }
};
void start(GoogleOperation &operation, Engine &engine, Completion &completion, qint64 pid = QCoreApplication::applicationPid()) {
    operation.start(engine.profile, pid, "google_calendar_write", {{"request_id", "fixture-request"}}, completion.callback());
}
void independentCachedQuery() {
    Engine engine;
    Completion done;
    GoogleOperation operation("fixture-account", "fixture-request", nullptr);
    start(operation, engine, done);
    await([&] { return engine.operations == 1; }, "Google operation reached server");
    require(done.calls == 0, "provider operation stays pending");
    QLocalSocket cached;
    QByteArray buffer;
    bool queried = false;
    QObject::connect(&cached, &QLocalSocket::connected, &cached, [&] {
        QString error;
        require(verifyEnginePeer(cached.socketDescriptor(), QCoreApplication::applicationPid(), error), "cached connection peer identity");
        cached.write("{\"version\":1,\"id\":\"hello\",\"method\":\"hello\",\"params\":{\"token\":\"synthetic-operation-token\"}}\n");
    });
    QObject::connect(&cached, &QLocalSocket::readyRead, &cached, [&] {
        buffer += cached.readAll();
        while (buffer.contains('\n')) {
            const auto end = buffer.indexOf('\n');
            const auto response = QJsonDocument::fromJson(buffer.left(end)).object();
            buffer.remove(0, end + 1);
            if (response["id"] == "hello")
                cached.write("{\"version\":1,\"id\":\"cached\",\"method\":\"status\",\"params\":{}}\n");
            else queried = response["id"] == "cached" && response["result"].toObject()["cached"].toBool();
        }
    });
    cached.connectToServer(engine.server.serverName());
    await([&] { return queried; }, "independent cached query completes while Google is held");
    require(engine.connections == 2 && engine.hellos == 2 && engine.queries == 1 && done.calls == 0, "independent authenticated connections");
    engine.complete();
    await([&] { return done.calls == 1 && engine.operationCloses == 1; }, "operation completes and closes its owner");
    require(done.ok && done.value["state"] == "acknowledged", "operation receipt delivered");
    operation.cancel(); start(operation, engine, done);
    QCoreApplication::processEvents();
    require(done.calls == 1 && engine.operations == 1, "completed operation callback and dispatch are one-shot");
}
void cancelClosesOwner() {
    Engine engine;
    Completion done;
    GoogleOperation operation("fixture-account", "fixture-request", nullptr);
    start(operation, engine, done);
    await([&] { return engine.operations == 1; }, "cancellable operation begins");
    operation.cancel();
    await([&] { return engine.operationCloses == 1; }, "explicit cancel closes the server owner");
    require(done.calls == 1 && !done.ok && done.value["code"] == "cancelled", "one explicit cancellation callback");
    operation.cancel(); start(operation, engine, done);
    QCoreApplication::processEvents();
    require(done.calls == 1 && engine.operations == 1, "cancel never replays the operation");
}
void refusals() {
    for (const auto mode : {Behavior::Malformed, Behavior::OutOfOrder, Behavior::Oversized, Behavior::WrongType, Behavior::Duplicate, Behavior::WrongMode}) {
        Engine engine(mode);
        Completion done;
        GoogleOperation operation("fixture-account", "fixture-request", nullptr);
        start(operation, engine, done);
        await([&] { return done.calls == 1; }, "invalid server reply terminates operation");
        if (mode == Behavior::Duplicate)
            require(done.ok || done.value["code"] == "invalid_response", "duplicate frames finish only once whether coalesced or separately delivered");
        else
            require(!done.ok && done.value["code"] == (mode == Behavior::WrongMode ? "profile_mode_mismatch" : "invalid_response"), "typed response refusal");
        operation.cancel(); QCoreApplication::processEvents();
        require(done.calls == 1 && engine.operations <= 1, "invalid reply callback exactly once and no replay");
        if (mode == Behavior::OutOfOrder || mode == Behavior::WrongMode) require(engine.operations == 0, "refused handshake sends no operation");
    }
    for (const bool mismatchedPeer : {false, true}) {
        Engine engine;
        Completion done;
        GoogleOperation operation("fixture-account", "fixture-request", nullptr);
        const auto wrongPid = QCoreApplication::applicationPid() + 100000;
        engine.discovery(wrongPid);
        start(operation, engine, done, mismatchedPeer ? wrongPid : QCoreApplication::applicationPid());
        await([&] { return done.calls == 1; }, "wrong process identity refused");
        require(!done.ok && done.value["code"] == (mismatchedPeer ? "engine_untrusted" : "engine_disconnected"), "discovery and native peer identity are both checked");
        require(engine.hellos == 0 && engine.operations == 0, "wrong PID receives neither token nor operation");
    }
    Engine shared;
    shared.exposeDiscovery();
    Completion denied;
    GoogleOperation untrusted("fixture-account", "fixture-request", nullptr);
    start(untrusted, shared, denied);
    require(denied.calls == 1 && !denied.ok && denied.value["code"] == "engine_untrusted", "unsafe discovery permissions rejected before connection");
    require(shared.connections == 0 && shared.hellos == 0 && shared.operations == 0, "shared discovery never sends the handshake");
}
void callbackMayDestroyOperation() {
    Engine engine;
    auto operation = std::make_unique<GoogleOperation>("fixture-account", "fixture-request", nullptr);
    int calls = 0;
    QJsonObject large{{"request_id", "fixture-request"}, {"body", QString(1024 * 1024, 'x')}};
    operation->start(engine.profile, QCoreApplication::applicationPid(), "google_calendar_write", large,
        [&](const QJsonObject &result, bool ok) {
            ++calls;
            require(!ok && result["code"] == "invalid_request", "oversized outgoing operation refused");
            operation.reset();
        });
    await([&] { return calls == 1; }, "callback may immediately destroy its operation");
    require(!operation && engine.operations == 0, "no access after callback destruction or oversized dispatch");
}
}
int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    independentCachedQuery();
    cancelClosesOwner();
    refusals();
    callbackMayDestroyOperation();
    qInfo() << "Google operation IPC checks passed: private discovery/peer identity, independent cached query, cancellation, response bounds/order/types, one-shot callbacks and callback destruction. Synthetic local IPC only; no app latency or provider claim.";
    return 0;
}
