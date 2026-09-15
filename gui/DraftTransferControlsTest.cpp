#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include <QGuiApplication>
#include <QJsonArray>
using namespace GoogleOperationFixture;

class DraftTransferControlsTest {
public:
static void run() {
    Engine fixture;
    MailBackend backend(fixture.profile,"unused-fixture-engine",nullptr,false,true);
    backend.m_connected=true; backend.m_account="fixture-account"; backend.m_activeDraft="source-draft";
    backend.m_enginePid=QCoreApplication::applicationPid();
    QJsonObject source{{"account","fixture-account"},{"draft_id","source-draft"},{"revision",1},{"mode","new"},
        {"body","Authored text"},{"subject","Preserve me"},{"from","me@example.test"},{"from_name","Me"},
        {"to_recipients",QJsonArray{}},{"cc_recipients",QJsonArray{}},{"bcc_recipients",QJsonArray{}},
        {"attachment_ids",QJsonArray{}},{"signature_text","Custom signature"},{"google",QJsonObject{{"revision",0},{"state","local"}}}};
    backend.m_draft=source.toVariantMap();
    auto account=[](const QString &id,const QString &email){return QVariantMap{{"id",id},{"email",email},{"name",id},{"lifecycle",QVariantMap{{"state","active"},{"revision",1}}}};};
    backend.m_allAccounts={account("fixture-account","me@example.test"),account("target-account","other@example.test")};
    require(backend.publishAccounts(backend.m_allAccounts),"two active accounts");
    QObject::disconnect(&backend.m_socket,SIGNAL(connected()),&backend,nullptr);
    backend.m_socket.connectToServer(fixture.server.serverName());
    await([&]{return backend.m_socket.state()==QLocalSocket::ConnectedState;},"cached socket");
    bool hello=false; backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto,bool ok){hello=ok;});
    await([&]{return hello;},"cached hello");
    struct Held { QPointer<QLocalSocket> peer; QString id; QJsonObject params; };
    QList<Held> transfers; int saves=0; QStringList loaded;
    QJsonObject target;
    fixture.customRequest=[&](QLocalSocket *peer,const QJsonObject &request){
        const auto method=request["method"].toString(),id=request["id"].toString(); const auto params=request["params"].toObject();
        if(method=="draft_transfer") { transfers.append({peer,id,params}); return true; }
        if(method=="draft_save") {
            require(request["account"]=="fixture-account" && params["expected_revision"]==source["revision"],"save retains source account and revision");
            for(const auto key:{"body","subject","to_recipients","cc_recipients","bcc_recipients","signature_text"}) source[key]=params[key];
            source["revision"]=source["revision"].toInteger()+1; ++saves; peer->write(reply(id,source));
        } else if(method=="accounts") peer->write(reply(id,{{"accounts",QJsonArray::fromVariantList(backend.m_allAccounts)}}));
        else if(method=="view_get") peer->write(reply(id,{{"revision",1},{"view",QJsonObject{{"folder","inbox"},{"selectedDraft","previous-draft"}}}}));
        else if(method=="draft_get") {
            loaded.append(params["draft_id"].toString());
            peer->write(reply(id,request["account"]=="target-account" ? target : source));
        } else if(method=="view_save") peer->write(reply(id,{{"revision",params["expected_revision"].toInteger()+1}}));
        else peer->write(reply(id,{}));
        return true;
    };
    backend.stageDraftRecipient("to_recipients","recipient@example.test");
    backend.editDraft("Latest human text"); backend.m_draftTimer.stop();
    backend.changeDraftAccount("target-account");
    await([&]{return transfers.size()==1;},"transfer starts after save");
    require(saves==1 && source["body"]=="Latest human text" && source["to_recipients"].toArray().size()==1,"unsaved body and recipient committed before transfer");
    const auto intent=transfers[0].params;
    require(intent["expected_revision"]==source["revision"] && intent["target_account"]=="target-account" && !intent["keep_original"].toBool(),"source revision and destination captured");
    require(backend.draftLocked() && backend.draftBusy(),"editor locked while transfer runs");
    backend.editDraft("Should not change"); require(backend.m_draft["body"]=="Latest human text","in-flight edit cannot be lost");
    transfers[0].peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",transfers[0].id},{"ok",false},{"error",QJsonObject{{"code","timeout"},{"message","Synthetic lost response"}}}}).toJson(QJsonDocument::Compact)+'\n');
    await([&]{return backend.draftTransfer()["retry"].toBool();},"lost response retains retry");
    require(backend.draftLocked() && !backend.draftBusy(),"unknown result protects source without claiming active work");
    backend.retryDraftTransfer(); await([&]{return transfers.size()==2;},"same transfer retried");
    require(transfers[1].params==intent,"exact request replay");
    target=source; target["account"]="target-account"; target["draft_id"]="destination-draft"; target["revision"]=1; target["from"]="other@example.test";
    QJsonObject receipt{{"account","fixture-account"},{"request_id",intent["request_id"]},{"source_draft_id","source-draft"},
        {"target_account","target-account"},{"target_draft_id","destination-draft"},{"draft",target},{"state","complete"}};
    transfers[1].peer->write(reply(transfers[1].id,receipt));
    await([&]{return backend.m_account=="target-account" && backend.m_activeDraft=="destination-draft";},"move opens destination despite old saved view");
    require(!loaded.contains("previous-draft"),"saved destination view cannot retarget the transfer");
    require(backend.m_draft["body"]=="Latest human text" && backend.m_draft["signature_text"]=="Custom signature","destination preserves current content");
    require(!backend.draftLocked() && backend.m_folder=="drafts","destination is editable in Drafts");
    require(backend.m_views["fixture-account"].toMap()["selectedDraft"].toString().isEmpty(),"retired source selection cleared");
    qInfo()<<"PASS: draft account selection, save-before-transfer, lock, exact retry and destination view restoration";
}
};
int main(int argc,char **argv){QGuiApplication app(argc,argv); DraftTransferControlsTest::run(); return 0;}
