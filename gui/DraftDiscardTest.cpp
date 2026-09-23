#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QFontDatabase>
#include <QGuiApplication>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
using namespace GoogleOperationFixture;
namespace {
QQuickItem *find(QQuickItem *item, const QString &name) {
    if (item->objectName() == name) return item;
    for (auto child : item->childItems()) if (auto found = find(child,name)) return found;
    return nullptr;
}
QJsonObject draft(const QString &account,const QString &id) {
    return {{"account",account},{"draft_id",id},{"mode","new"},{"revision",4},{"discarded",false},
        {"from",account+"@example.test"},{"from_name","Frozen author"},{"identity_revision",3},{"subject","Draft "+id},
        {"body","Saved human words"},{"body_html",QJsonValue::Null},{"signature_text","My signature"},{"updated_at",1800000000},
        {"to_recipients",QJsonArray{QJsonObject{{"email","reader@example.test"},{"name","Reader"}}}},{"cc_recipients",QJsonArray{}},{"bcc_recipients",QJsonArray{}},
        {"recipient_inputs",QJsonObject{}},{"attachment_ids",QJsonArray{"retained-file"}},
        {"attachments",QJsonArray{QJsonObject{{"id","retained-file"},{"name","notes.txt"},{"state","available"},{"size",12}}}}};
}
void reject(QLocalSocket *peer,const QString &id,const QString &code) {
    peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",id},{"ok",false},{"error",QJsonObject{{"code",code},{"message","Isolated fixture interruption"}}}}).toJson(QJsonDocument::Compact)+'\n');
}
}
class DraftDiscardTest {
public:
static void run() {
    for (const auto weight : {"Regular","Medium","SemiBold"}) require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR)+"/fonts/Geist-"+weight+".ttf")>=0,"bundled font");
    Engine fixture; MailBackend backend(fixture.profile,"unused-fixture-engine",nullptr,false,true);
    backend.m_connected=true; backend.m_account="fixture-account"; backend.m_activeDraft="draft-a";backend.m_folder="drafts";backend.m_enginePid=QCoreApplication::applicationPid();
    auto account=[](const QString &id){return QVariantMap{{"id",id},{"name",id},{"email",id+"@example.test"},{"lifecycle",QVariantMap{{"state","active"},{"revision",1}}},
        {"google",QVariantMap{{"capabilities",QVariantMap{{"mail_read",true},{"mail_write",true},{"send",true}}}}}};};
    backend.m_allAccounts={account("fixture-account"),account("other-account")};require(backend.publishAccounts(backend.m_allAccounts),"account list");
    QHash<QString,QJsonObject> drafts,receipts,snapshots;
    drafts["draft-a"]=draft("fixture-account","draft-a");drafts["draft-b"]=draft("fixture-account","draft-b");drafts["other-draft"]=draft("other-account","other-draft");
    backend.m_draft=drafts["draft-a"].toVariantMap();
    QObject::disconnect(&backend.m_socket,SIGNAL(connected()),&backend,nullptr);backend.m_socket.connectToServer(fixture.server.serverName());
    await([&]{return backend.m_socket.state()==QLocalSocket::ConnectedState;},"cached connection");
    bool hello=false;backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto,bool ok){hello=ok;});await([&]{return hello;},"cached hello");
    QList<QJsonObject> discards,undos;int saves=0,sends=0;bool loseDiscard=false,loseUndo=false;
    auto listDrafts=[&](const QString &a){QJsonArray rows;for(const auto &d:drafts)if(d["account"]==a&&!d["discarded"].toBool())rows.append(d);return rows;};
    fixture.customRequest=[&](QLocalSocket *peer,const QJsonObject &request){
        const auto method=request["method"].toString(),id=request["id"].toString(),a=request["account"].toString();const auto p=request["params"].toObject();
        const auto did=p["draft_id"].toString(a=="other-account"?"other-draft":"draft-a"),rid=p["request_id"].toString();
        if(method=="google_send") {++sends;reject(peer,id,"invalid_draft");return true;}
        if(method=="draft_discard"||method=="draft_undo_discard") {
            (method=="draft_discard"?discards:undos).append(p);
            if(!receipts.contains(rid)) {
                require(drafts.contains(did)&&drafts[did]["account"]==a,"request stays on original account/draft");
                if(p["expected_revision"]!=drafts[did]["revision"]) {reject(peer,id,"stale_revision");return true;}
                auto value=drafts[did];
                if(method=="draft_discard") {
                    auto snapshot=value;const auto buffer=p["buffer"].toObject();for(auto it=buffer.begin();it!=buffer.end();++it)snapshot[it.key()]=it.value();snapshots[rid]=snapshot;
                    value["body"]="";value["body_html"]=QJsonValue::Null;value["signature_text"]="";value["attachment_ids"]=QJsonArray{};value["attachments"]=QJsonArray{};value["recipient_inputs"]=QJsonObject{};value["discarded"]=true;
                } else {value=snapshots[p["discard_request_id"].toString()];value["discarded"]=false;value["undone_discard_request_id"]=p["discard_request_id"];}
                value["revision"]=p["expected_revision"].toInteger()+1;drafts[did]=value;
                if(method=="draft_discard") value["discard_undo"]=QJsonObject{{"version",1},{"request_id",rid},{"draft_id",did},{"revision",value["revision"]},{"lifecycle_revision",1}};
                receipts[rid]=value;
            }
            if(method=="draft_discard"&&loseDiscard){loseDiscard=false;reject(peer,id,"timeout");}
            else if(method=="draft_undo_discard"&&loseUndo){loseUndo=false;reject(peer,id,"timeout");}
            else peer->write(reply(id,receipts[rid]));return true;
        }
        if(method=="draft_discard_status") {
            if(!receipts.contains(rid)){reject(peer,id,"not_found");return true;}
            const auto receipt=receipts[rid],current=drafts[receipt["draft_id"].toString()];
            peer->write(reply(id,{{"account",a},{"request_id",rid},{"draft_id",receipt["draft_id"]},{"discard",receipt},{"current_revision",current["revision"]},{"can_undo",current["discarded"].toBool()&&current["revision"]==receipt["revision"]}}));return true;
        }
        if(method=="draft_get") peer->write(reply(id,drafts[did]));
        else if(method=="draft_save") {
            require(p["expected_revision"]==drafts[did]["revision"],"autosave uses current revision");auto value=drafts[did];
            for(const auto field:{"body","body_html","signature_text","subject","to_recipients","cc_recipients","bcc_recipients","attachment_ids","recipient_inputs"})value[field]=p[field];
            value["revision"]=value["revision"].toInteger()+1;drafts[did]=value;++saves;peer->write(reply(id,value));
        } else if(method=="accounts")peer->write(reply(id,{{"accounts",QJsonArray::fromVariantList(backend.m_allAccounts)}}));
        else if(method=="drafts") {const auto rows=listDrafts(a);peer->write(reply(id,{{"drafts",rows},{"total",rows.size()},{"next_offset",QJsonValue::Null}}));}
        else if(method=="view_get")peer->write(reply(id,{{"revision",1},{"view",QJsonObject{{"folder","drafts"},{"selectedDraft",a=="other-account"?"other-draft":"draft-a"},{"replyOpen",true}}}}));
        else if(method=="view_save")peer->write(reply(id,{{"revision",p["expected_revision"].toInteger()+1}}));
        else if(method=="assist_settings_get")peer->write(reply(id,{{"revision",0},{"settings",QJsonObject{{"accounts",QJsonObject{}},{"model",QJsonValue::Null}}}}));
        else peer->write(reply(id,{}));return true;
    };
    QTemporaryDir copy;const auto modulePath=copy.filePath("DiscardTest");require(QDir().mkpath(modulePath),"module directory");QByteArray module("module DiscardTest\nsingleton Theme 1.0 Theme.qml\n");
    for(const auto &name:QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"},QDir::Files)) {
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name),"production QML");if(name=="ShortcutRegistry.qml") module+="singleton ";
            if(name!="Theme.qml") module+=name.chopped(4).toUtf8()+" 1.0 "+name.toUtf8()+'\n';
    }
    QFile registration(modulePath+"/qmldir");require(registration.open(QIODevice::WriteOnly),"module registration");registration.write(module);registration.close();
    qmlRegisterType<ReplyDocument>("DiscardTest",1,0,"ReplyDocument");QQmlApplicationEngine qml;bool warnings=false;
    QObject::connect(&qml,&QQmlEngine::warnings,[&](const QList<QQmlError> &errors){warnings=true;for(const auto &error:errors)qWarning()<<error;});
    qml.addImportPath(copy.path());qml.rootContext()->setContextProperty("backend",&backend);
    qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import DiscardTest 1.0
ApplicationWindow {width:1280;height:800;visible:true;color:Theme.bg
 property bool singleKeysAllowed:true;property bool searchPopupOpen:false
 function appearance(theme){Theme.themeId=theme}
 MailView {anchors.fill:parent}
 ActionToast {anchors.horizontalCenter:parent.horizontalCenter;anchors.bottom:parent.bottom;anchors.bottomMargin:16}
})QML",QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(),"mail workspace loads");auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());require(window,"window");await([&]{return window->isExposed();},"window exposed");
    auto mail=find(window->contentItem(),"mailWorkspace");require(mail,"real mail view");
    auto click=[&](const QString &name){auto item=find(window->contentItem(),name);require(item&&item->isVisible()&&item->isEnabled(),"visible enabled native control");require(QMetaObject::invokeMethod(item,"clicked"),"real control handler");};
    backend.acceptDraft(drafts["draft-a"]);await([&]{return mail->property("replyOpen").toBool();},"composer opened");
    backend.editDraft("Unsaved human words");backend.m_draft["body_html"]="<p>Unsaved <b>human words</b></p>";backend.stageDraftRecipient("bcc_recipients","Unfinished <address@");backend.m_draftTimer.stop();
    const auto authored=backend.discardEditorBuffer();const auto savesBefore=saves;click("discardReply");
    await([&]{return find(window->contentItem(),"confirmDiscardDraft");},"shared discard confirmation");require(discards.isEmpty(),"confirmation precedes discard");
    const auto captures=qEnvironmentVariable("FM_DISCARD_SCREENSHOTS");
    if(!captures.isEmpty()){QDir().mkpath(captures);for(const auto theme:{"graphite","paper","terminal"}){QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)));QCoreApplication::processEvents();require(window->grabWindow().save(captures+"/discard-confirm-"+theme+".png"),"confirmation theme capture");}}
    click("confirmDiscardDraft");await([&]{return discards.size()==1&&!backend.draftDiscardPending();},"discard commits");
    await([&]{return !mail->property("replyOpen").toBool();},"discard closes composer");require(discards[0]["buffer"].toObject()==authored&&saves==savesBefore,"captured buffer includes unfinished recipients without forced save");
    backend.selectAccount("other-account");await([&]{return backend.m_activeDraft=="other-draft";},"browse another account during Undo");
    backend.editDraft("Other mailbox words");backend.stageDraftRecipient("cc_recipients","Still typing <");backend.m_draftTimer.stop();
    click("actionToastButton");await([&]{return !backend.m_undoingDiscards&&backend.m_account=="fixture-account"&&backend.m_activeDraft=="draft-a"&&mail->property("replyOpen").toBool();},"Undo returns to original composer");
    require(drafts["other-draft"]["body"]=="Other mailbox words"&&drafts["other-draft"]["recipient_inputs"].toObject()["cc_recipients"]=="Still typing <","other editor saved before returning");
    require(backend.m_draft["body"]==authored["body"].toVariant()&&backend.m_draft["body_html"]==authored["body_html"].toVariant()&&backend.m_draft["attachment_ids"]==authored["attachment_ids"].toVariant(),"restored words formatting and file selection");
    auto pending=find(window->contentItem(),"draftRecipient_bcc_recipients");require(pending&&pending->isVisible()&&pending->property("text")=="Unfinished <address@","unfinished Bcc restored in visible input");
    backend.sendGoogleDraft();require(sends==0&&!backend.sendQueued(),"unfinished input never arms Send");backend.clearError();
    loseDiscard=true;backend.discardDraft();await([&]{return backend.draftDiscard()["can_check"].toBool();},"lost discard result retained");
    const auto lostDiscard=discards.last();require(backend.draftLocked(),"unknown discard locks original buffer");backend.shutdown();require(!backend.m_shutdown,"unknown action prevents losing recovery on close");backend.clearError();
    click("checkDraftDiscard");await([&]{return !backend.draftDiscardPending();},"cached status finds committed discard");loseUndo=true;click("actionToastButton");
    await([&]{return !backend.m_undoingDiscards&&backend.draftDiscard()["can_retry"].toBool();},"lost Undo retains exact request");const auto lostUndo=undos.last();
    click("actionToastButton");await([&]{return mail->property("replyOpen").toBool()&&find(window->contentItem(),"retryDraftDiscard")->isVisible();},"recovery opens original draft");
    click("retryDraftDiscard");await([&]{return !backend.draftDiscardPending()&&backend.m_inFlight.isEmpty();},"exact Undo retry completes");
    require(undos.last()==lostUndo&&discards.contains(lostDiscard),"recovery keeps original identities");
    click("discardReply");await([&]{return find(window->contentItem(),"confirmDiscardDraft")->isVisible();},"confirmation reopened");const auto priorDiscards=discards.size();backend.acceptDraft(drafts["draft-b"]);click("confirmDiscardDraft");
    require(discards.size()==priorDiscards&&!drafts["draft-b"]["discarded"].toBool(),"stale confirmation cannot target another draft");backend.clearError();
    backend.acceptDraft(drafts["draft-a"]);backend.loadDraftRows(false);await([&]{return backend.m_conversations.size()>=2;},"actual draft list loaded");
    const auto rows=backend.snapshotMailRows({"draft-a","draft-b"});require(rows.size()==2,"two exact draft snapshots");backend.applyMailSelection(rows,"discard");
    await([&]{return !backend.m_mutatingConversation&&drafts["draft-a"]["discarded"].toBool()&&drafts["draft-b"]["discarded"].toBool();},"both selected drafts discarded");
    require(backend.actionToast()["text"]=="2 drafts discarded"&&backend.m_actionToast["drafts"].toList().size()==2,"one grouped Undo");click("actionToastButton");
    await([&]{return !backend.m_undoingDiscards&&!drafts["draft-a"]["discarded"].toBool()&&!drafts["draft-b"]["discarded"].toBool();},"group restores both drafts");
    await([&]{return backend.m_inFlight.isEmpty();},"group readback complete");backend.acceptDraft(drafts["draft-a"]);backend.discardDraft();await([&]{return backend.actionToast()["action_label"]=="Undo";},"final Undo interval");
    const auto deadline=backend.m_actionToastDeadline;const auto undoCount=undos.size();QElapsedTimer elapsed;elapsed.start();
    while(backend.actionToast()["visible"].toBool()&&elapsed.elapsed()<6500)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
    require(backend.m_actionClock.elapsed()>=deadline&&!backend.actionToast()["visible"].toBool(),"real six-second discard Undo expires");backend.activateActionToast();require(undos.size()==undoCount,"expired toast cannot restore");
    drafts["close-a"]=draft("fixture-account","close-a");drafts["close-b"]=draft("fixture-account","close-b");
    backend.acceptDraft(drafts["close-a"]);backend.loadDraftRows(false);await([&]{return backend.snapshotMailRows({"close-a","close-b"}).size()==2;},"close group loaded");
    backend.applyMailSelection(backend.snapshotMailRows({"close-a","close-b"}),"discard");
    await([&]{return !backend.m_mutatingConversation&&drafts["close-a"]["discarded"].toBool()&&drafts["close-b"]["discarded"].toBool();},"close group discarded");
    backend.activateActionToast();require(backend.m_undoingDiscards,"group Undo accepted before close");backend.shutdown();
    await([&]{return backend.m_shutdownEmitted;},"close waits for accepted group Undo");
    require(!drafts["close-a"]["discarded"].toBool()&&!drafts["close-b"]["discarded"].toBool(),"both drafts restored before engine shutdown");
    require(!warnings,"production QML has no warnings");qInfo()<<"PASS: draft discard/Undo, complete editor buffer, cross-account navigation, incomplete recipient autosave, frozen confirmation, exact lost-result recovery, grouped selection and six-second expiry";
}
};
int main(int argc,char **argv){QGuiApplication app(argc,argv);QQuickStyle::setStyle("Basic");DraftDiscardTest::run();return 0;}
