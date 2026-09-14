#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include <QGuiApplication>
#include <QJsonArray>

using namespace GoogleOperationFixture;

class CalendarOccurrenceBackendTest {
public:
    static void run() {
        Engine fixture; MailBackend backend(fixture.profile, "unused", nullptr, false, true);
        backend.m_connected = true; backend.m_syntheticProfile = false;
        backend.m_enginePid = QCoreApplication::applicationPid(); backend.m_account = "another-mailbox";
        QVariantMap account{{"id", "fixture-account"}, {"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}},
            {"google", QVariantMap{{"capabilities", QVariantMap{{"calendar_write", true}}}}}};
        backend.m_allAccounts = {account}; backend.publishAccounts({account});
        backend.m_googleSync = {{"fixture-account", QVariantMap{{"active", true}}}};
        QVariantMap source{{"account", "fixture-account"}, {"calendar_id", "calendar"}, {"writable", true}, {"revision", 1}, {"permission", "owner"}, {"time_zone", "America/Toronto"}};
        backend.m_calendarSources = {source};
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(fixture.server.serverName());
        await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached connection");
        bool hello = false; backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
        await([&] { return hello; }, "cached authentication");
        fixture.expectedMethod = "google_calendar_occurrence_read";
        const QJsonObject original{{"kind", "zoned"}, {"instant", "2026-09-13T13:00:00Z"}, {"time_zone", "America/Toronto"}};
        fixture.expectedParams = {{"calendar_id", "calendar"}, {"series_id", "master"}, {"original_start", original}};
        auto selection = fixture.expectedParams.toVariantMap(); selection["account"] = "fixture-account";
        const QJsonObject occurrence{{"series_id", "master"}, {"original_start", original}, {"expected_master_etag", "master-v1"}};
        const QJsonObject event{{"account", "fixture-account"}, {"calendar_id", "calendar"}, {"event_id", "opaque-instance"}, {"etag", "instance-v1"}, {"can_edit", true}, {"can_delete", true}};
        const QJsonObject valid{{"account", "fixture-account"}, {"calendar_id", "calendar"}, {"event_id", "opaque-instance"}, {"etag", "instance-v1"}, {"occurrence", occurrence}, {"event", event}};
        int completions = 0; bool lastOk = false; QString lastCorrelation; QVariantMap lastResult;
        QObject::connect(&backend, &MailBackend::calendarOccurrenceReadFinished, [&](const QString &token, bool ok, const QVariantMap &result, const QString &) {
            ++completions; lastCorrelation = token; lastOk = ok; lastResult = result;
        });
        const auto begin = [&](const QString &token) {
            const int next = fixture.operations + 1;
            backend.readCalendarOccurrence(selection, token);
            await([&] { return fixture.operations == next; }, "owned occurrence read starts");
            require(fixture.operationParams == fixture.expectedParams && !fixture.operationParams.contains("request_id"), "correlation stays local and master ID is not an instance hint");
        };
        const auto finish = [&](const QJsonObject &response, bool expected) {
            const int next = completions + 1; fixture.complete(response);
            await([&] { return completions == next && backend.m_googleOperations.isEmpty(); }, "owned occurrence read completes");
            require(lastOk == expected && (expected || lastResult.isEmpty()), "only validated resolved identity reaches controls");
        };
        begin("first"); bool cached = false;
        backend.request("status", {}, {}, [&](auto result, bool ok) { cached = ok && result["cached"].toBool(); });
        await([&] { return cached; }, "held provider resolution leaves cached queries free");
        finish(valid, true); require(lastCorrelation == "first" && backend.m_account == "another-mailbox", "resolved event retains its account without mailbox selection");
        for (const auto field : {"account", "calendar_id", "event_id", "etag"}) {
            begin(QString("wrong-") + field); auto wrong = valid; wrong[field] = "different"; finish(wrong, false);
        }
        begin("date-type"); auto wrong = valid; auto tuple = occurrence; tuple["original_start"] = QJsonObject{{"kind", "date"}, {"date", "2026-09-13"}}; wrong["occurrence"] = tuple; finish(wrong, false);
        begin("same-instant"); auto equivalent = valid; tuple = occurrence; auto offset = original; offset["instant"] = "2026-09-13T09:00:00-04:00"; tuple["original_start"] = offset; equivalent["occurrence"] = tuple; finish(equivalent, true);
        begin("cancelled"); wrong = valid; auto cancelled = event; cancelled["cancelled"] = true; wrong["event"] = cancelled; finish(wrong, false);
        const QJsonObject allDay{{"kind", "date"}, {"date", "2026-09-13"}};
        fixture.expectedParams["original_start"] = allDay; selection["original_start"] = allDay.toVariantMap();
        begin("all-day"); auto dated = valid; tuple = occurrence; tuple["original_start"] = allDay; dated["occurrence"] = tuple;
        auto dateEvent = event; dateEvent["all_day"] = true; dated["event"] = dateEvent; finish(dated, true);
        fixture.expectedParams["original_start"] = original; selection["original_start"] = original.toVariantMap();
        fixture.expectedParams["scope"] = "series"; fixture.expectedParams["action"] = "rsvp";
        selection["scope"] = "series"; selection["action"] = "rsvp";
        auto scoped = valid; scoped.remove("occurrence");
        scoped["recurring"] = QJsonObject{{"scope", "series"}, {"series_id", "master"}, {"expected_master_etag", "master-v1"}};
        scoped["event_id"] = "master"; scoped["etag"] = "master-v1";
        auto master = event; master["event_id"] = "master"; master["etag"] = "master-v1"; master["can_edit"] = false; master["can_delete"] = false; master["can_rsvp"] = true; scoped["event"] = master;
        begin("series-rsvp"); finish(scoped, true);
        begin("wrong-scope"); wrong = scoped; auto wrongScope = scoped["recurring"].toObject(); wrongScope["scope"] = "occurrence"; wrong["recurring"] = wrongScope; finish(wrong, false);
        begin("unconfirmed-reset"); wrong = scoped; wrongScope = scoped["recurring"].toObject(); wrongScope["reset_future_exceptions"] = true; wrong["recurring"] = wrongScope; finish(wrong, false);
        begin("rsvp-denied"); wrong = scoped; auto denied = master; denied["can_rsvp"] = false; denied["can_edit"] = true; wrong["event"] = denied; finish(wrong, false);
        begin("wrong-master-target"); wrong = scoped; wrong["event_id"] = "opaque-instance"; finish(wrong, false);
        fixture.expectedParams.remove("scope"); fixture.expectedParams.remove("action"); selection.remove("scope"); selection.remove("action");
        begin("epoch"); account["lifecycle"] = QVariantMap{{"state", "active"}, {"revision", 3}}; backend.m_allAccounts = {account}; backend.publishAccounts({account}); finish(valid, false);
        begin("source"); source["revision"] = 2; backend.m_calendarSources = {source}; finish(valid, false);
        // Reconciliation is a read against the exact recorded target. The
        // shared engine remains authoritative for its current detail access.
        fixture.customRequest = [](QLocalSocket *peer, const QJsonObject &request) {
            if (request["method"] != "calendar_operations") return false;
            peer->write(reply(request["id"].toString(), {{"operations", QJsonArray{}}})); return true;
        };
        source["writable"] = false; source["permission"] = "reader"; backend.m_calendarSources = {source};
        account["google"] = QVariantMap{{"capabilities", QVariantMap{{"calendar_read", true}, {"calendar_write", false}}}};
        backend.m_allAccounts = {account}; backend.publishAccounts({account});
        const int beforeRecovery = fixture.operations;
        backend.calendarOperationAction("fixture-account", "fixture-request", "resume", 3);
        require(fixture.operations == beforeRecovery && backend.m_calendarWrites == 0, "Read-only grant cannot Resume a provider mutation");
        await([&] { return !backend.m_calendarOperationsLoading; }, "refusal history refresh completes");
        fixture.expectedMethod = "google_calendar_reconcile"; fixture.expectedParams = {{"request_id", "fixture-request"}};
        int recoveryReplies = 0; QVariantMap recovered;
        QObject::connect(&backend, &MailBackend::calendarWriteFinished, [&](bool, const QVariantMap &result, const QString &) { ++recoveryReplies; recovered = result; });
        backend.calendarOperationAction("fixture-account", "fixture-request", "reconcile", 3);
        await([&] { return fixture.operations == beforeRecovery + 1; }, "Read-only grant admits exact owned reconciliation");
        fixture.complete({{"state", "unknown"}, {"request_id", "fixture-request"}});
        await([&] { return recoveryReplies == 1 && backend.m_googleOperations.isEmpty() && !backend.m_calendarOperationsLoading; }, "Read-only reconciliation retains exact unknown result");
        require(recovered["operation_action"] == "reconcile" && recovered["state"] == "unknown", "Read inspection never promotes provider certainty");
        account["google"] = QVariantMap{{"capabilities", QVariantMap{{"calendar_read", false}, {"calendar_write", false}}}};
        backend.m_allAccounts = {account}; backend.publishAccounts({account});
        backend.calendarOperationAction("fixture-account", "fixture-request", "reconcile", 3);
        require(fixture.operations == beforeRecovery + 1, "Missing detail-read grant refuses reconciliation before owned connection");
        await([&] { return !backend.m_calendarOperationsLoading; }, "read refusal history refresh completes");
        const int readsBeforeArchive = fixture.operations;
        account["lifecycle"] = QVariantMap{{"state", "removed"}, {"revision", 4}}; backend.m_allAccounts = {account}; backend.publishAccounts({});
        backend.readCalendarOccurrence(selection, "archived");
        require(!lastOk && lastCorrelation == "archived" && fixture.operations == readsBeforeArchive, "archive refuses resolution before creating an owner");
        backend.m_connected = false; backend.m_socket.abort();
        qInfo() << "Occurrence backend passed: private owned IPC, exact typed identity, independent cached progress, lifecycle/source fences and archive refusal. Fixture transport only.";
    }
};

int main(int argc, char **argv) { QGuiApplication app(argc, argv); CalendarOccurrenceBackendTest::run(); return 0; }
