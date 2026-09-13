#include "GoogleOperationTestFixture.h"
using namespace GoogleOperationFixture;
namespace {
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
void localImportProfileModes() {
    for (const auto &mode : {QString("synthetic"), QString("live"), QString("unexpected")}) {
        Engine engine; engine.profileMode=mode; engine.expectedMethod="draft_attachment_import";
        Completion done; GoogleOperation operation("fixture-account","fixture-request",nullptr);
        operation.start(engine.profile,QCoreApplication::applicationPid(),"draft_attachment_import",
            {{"request_id","fixture-request"},{"draft_id","draft-fixture"},{"expected_revision",1},{"path","/fixture/file.bin"}},done.callback());
        if (mode=="unexpected") {
            await([&] { return done.calls==1; },"unknown local profile mode refused");
            require(!done.ok && engine.operations==0,"invalid local profile cannot import");
        } else {
            await([&] { return engine.operations==1; },"local import accepts the explicit synthetic/live profile");
            engine.complete(); await([&] { return done.calls==1; },"local import result delivered");
            require(done.ok,"local import transport keeps provider-independent profile support");
        }
    }
}
void localSourceProfileModes() {
    for (const auto &mode : {QString("synthetic"), QString("live"), QString("unexpected")}) {
        Engine engine; engine.profileMode=mode; engine.expectedMethod="google_draft_source";
        engine.expectedParams={{"draft_id","remote-draft"},{"source_revision",3},{"attachment_id","retained-part"}};
        Completion done; GoogleOperation operation("fixture-account","",nullptr);
        operation.start(engine.profile,QCoreApplication::applicationPid(),"google_draft_source",engine.expectedParams,done.callback());
        if(mode=="unexpected") {
            await([&]{return done.calls==1;},"invalid source-export profile refused");
            require(!done.ok && engine.operations==0,"unknown profile sends no source request");
        } else {
            await([&]{return engine.operations==1;},"verified source export supports cached live/synthetic profiles");
            engine.complete(); await([&]{return done.calls==1;},"source export returns once"); require(done.ok,"local source export completed");
        }
    }
}
void exactDraftRecoveryMethods() {
    for (const auto &method : {QString("google_draft_reconcile"), QString("google_draft_resolve")}) {
        Engine engine; engine.expectedMethod = method;
        engine.expectedParams = {{"draft_id", "draft-fixture"}};
        if (method == "google_draft_resolve") engine.expectedParams = {{"draft_id", "draft-fixture"}, {"expected_revision", 3},
            {"expected_google_revision", 7}, {"request_id", "fixture-request"}, {"action", "replace_local"}};
        Completion done; GoogleOperation operation("fixture-account", "fixture-request", nullptr);
        operation.start(engine.profile, QCoreApplication::applicationPid(), method, engine.expectedParams, done.callback());
        await([&] { return engine.operations == 1; }, "exact draft recovery method dispatched");
        engine.complete(); await([&] { return done.calls == 1; }, "draft recovery completed once");
        require(done.ok, "draft recovery accepted direct status");
    }
    for (const auto &method : {QString("google_draft_send"), QString("google_draft_delete"), QString("google_draft_resolve_extra")}) {
        Engine engine; Completion done; GoogleOperation operation("fixture-account", "fixture-request", nullptr);
        operation.start(engine.profile, QCoreApplication::applicationPid(), method, {}, done.callback());
        require(done.calls == 1 && !done.ok && done.value["code"] == "invalid_request" && engine.connections == 0,
            "draft allowlist rejects extra endpoints before connection");
    }
}
void exactSendMethods() {
    for (const auto &method : {QString("google_send"), QString("google_send_reconcile"), QString("google_send_abandon")}) {
        Engine engine; engine.expectedMethod = method;
        engine.expectedParams = {{"request_id", "fixture-request"}};
        if (method == "google_send") engine.expectedParams = {{"draft_id", "draft-fixture"}, {"expected_revision", 3},
            {"expected_google_revision", 0}, {"request_id", "fixture-request"}};
        if (method == "google_send_abandon") engine.expectedParams["expected_revision"] = 4;
        Completion done; GoogleOperation operation("fixture-account", "fixture-request", nullptr);
        operation.start(engine.profile, QCoreApplication::applicationPid(), method, engine.expectedParams, done.callback());
        await([&] { return engine.operations == 1; }, "exact Send operation method dispatched");
        engine.complete(); await([&] { return done.calls == 1; }, "Send operation completes once");
        require(done.ok, "exact Send method accepts direct status");
    }
    Engine engine; Completion done; GoogleOperation operation("fixture-account", "fixture-request", nullptr);
    operation.start(engine.profile, QCoreApplication::applicationPid(), "google_send_retry_any", {}, done.callback());
    require(done.calls == 1 && !done.ok && engine.connections == 0, "Send methods do not enable wildcard retries");
}
}
int main(int argc, char **argv) {
    QCoreApplication app(argc, argv);
    independentCachedQuery();
    cancelClosesOwner();
    refusals();
    callbackMayDestroyOperation();
    localImportProfileModes();
    localSourceProfileModes();
    exactDraftRecoveryMethods();
    exactSendMethods();
    qInfo() << "Google operation IPC checks passed: private discovery/peer identity, independent cached query, cancellation, response bounds/order/types, one-shot callbacks and callback destruction. Synthetic local IPC only; no app latency or provider claim.";
    return 0;
}
