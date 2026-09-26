// Backend test for the calendar grid's per-day mail counts on a real MailBackend
// over the scripted engine: account and date scoping, refresh throttling on
// revision changes, and discarding stale replies once the counts are hidden.
#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include <QGuiApplication>
#include <QJsonArray>
using namespace GoogleOperationFixture;

class CalendarMailActivityTest {
public:
    static void run() {
        Engine fixture; MailBackend backend(fixture.profile, "unused", nullptr, false, true);
        backend.m_connected = true; backend.m_account = "alpha";
        backend.m_calendarDate = "2026-09-14"; backend.m_calendarView = "week";
        backend.m_preferences = {{"calendar_display_time_zone","UTC"}};
        backend.m_diagnostics = {{"revision",1}};
        backend.m_accounts = {QVariantMap{{"id","alpha"}},QVariantMap{{"id","beta"}}};
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(fixture.server.serverName());
        await([&] {return backend.m_socket.state()==QLocalSocket::ConnectedState;},"socket");
        bool hello=false; backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto,bool ok){hello=ok;});
        await([&]{return hello;},"hello");
        struct Held { QPointer<QLocalSocket> peer; QJsonObject request; };
        QList<Held> held;
        fixture.customRequest = [&](QLocalSocket *peer,const QJsonObject &request) {
            require(request["method"]=="calendar_mail_activity","count observer issues only a cached count query");
            held.append({peer,request});return true;
        };
        auto result = [&](int index,int received) {
            const auto request=held[index].request; const auto params=request["params"].toObject();
            const auto from=QDate::fromString(params["from"].toString(),Qt::ISODate), to=QDate::fromString(params["to"].toString(),Qt::ISODate);
            QJsonArray days;for(auto date=from;date<to;date=date.addDays(1)) days.append(QJsonObject{{"date",date.toString(Qt::ISODate)},{"received",received},{"sent",2}});
            return QJsonObject{{"from",params["from"]},{"to",params["to"]},{"account",request.contains("account")?request["account"]:QJsonValue(QJsonValue::Null)},{"time_zone","UTC"},{"state","ready"},{"days",days},{"cached_only",true}};
        };
        auto complete=[&](int index,QJsonObject value){held[index].peer->write(reply(held[index].request["id"].toString(),value));};
        backend.setCalendarMailActivityContext(true,true);await([&]{return held.size()==1;},"combined request");
        backend.setCalendarMailActivityContext(true,false);
        complete(0,result(0,90));await([&]{return held.size()==2;},"scoped request after held combined response");
        require(backend.calendarMailActivity()["state"]=="loading","old combined counts cannot flash in selected account");
        require(held[1].request["account"]=="alpha","selected account scope");
        complete(1,result(1,7));await([&]{return backend.calendarMailActivity()["state"]=="ready";},"current counts");
        require(backend.calendarMailActivity()["days"].toList().first().toMap()["received"]==7,"current count published");
        QMetaObject::invokeMethod(&backend.m_calendarMailActivityTimer,"timeout");require(held.size()==2,"unchanged revision issues no additional query");
        backend.m_diagnostics["revision"]=2;emit backend.preferencesChanged();require(held.size()==2,"background revisions wait for count refresh interval");
        QMetaObject::invokeMethod(&backend.m_calendarMailActivityTimer,"timeout");await([&]{return held.size()==3;},"changed store refreshes counts");
        auto invalid=result(2,8);invalid["from"]="2026-09-01";complete(2,invalid);
        await([&]{return backend.calendarMailActivity()["state"]=="unavailable";},"wrong date response cannot publish");
        backend.m_calendarDate="2026-10-01";emit backend.eventsChanged();await([&]{return held.size()==4;},"navigation refreshes immediately");
        backend.setCalendarMailActivityContext(false,false);complete(3,result(3,11));
        await([&]{return backend.m_pending.isEmpty();},"drain hidden reply");
        require(backend.calendarMailActivity().isEmpty() && !backend.m_calendarMailActivityTimer.isActive(),"hidden counts stop and stale responses are fenced");
        qInfo()<<"Calendar mail count backend passed: account/date fences, revision refresh, cached-only requests and hidden lifetime.";
    }
};
int main(int argc,char **argv){QGuiApplication app(argc,argv);CalendarMailActivityTest::run();}
