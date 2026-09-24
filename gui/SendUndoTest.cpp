#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
using namespace GoogleOperationFixture;
namespace {
QQuickItem *find(QQuickItem *item,const QString &name) {
    if(item->objectName()==name) return item;
    for(auto child:item->childItems()) if(auto found=find(child,name)) return found;
    return nullptr;
}
QJsonObject draft(const QString &account,const QString &id) {
    return {{"account",account},{"draft_id",id},{"mode","new"},{"revision",5},{"discarded",false},
        {"from",account+"@example.test"},{"from_name","Author"},{"identity_revision",1},{"subject","Human subject"},
        {"body","Exact human body"},{"body_html",QJsonValue::Null},{"signature_text","Signature"},
        {"to_recipients",QJsonArray{QJsonObject{{"email","reader@example.test"},{"name","Reader"}}}},
        {"cc_recipients",QJsonArray{}},{"bcc_recipients",QJsonArray{}},{"attachment_ids",QJsonArray{}},{"attachments",QJsonArray{}},
        {"google",QJsonObject{{"account",account},{"draft_id",id},{"revision",0},{"state","local"},{"local_revision",5},{"operation_is_current",true}}}};
}
struct Held { QPointer<QLocalSocket> peer; QString id,account; QJsonObject params; };
}
class SendUndoTest {
public:
static void run() {
    for(const auto weight:{"Regular","Medium","SemiBold"})
        require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR)+"/fonts/Geist-"+weight+".ttf")>=0,"bundled font");
    Engine fixture; MailBackend backend(fixture.profile,"unused-fixture-engine",nullptr,false,true);
    backend.m_connected=true; backend.m_googleSync["fixture-account"] = QVariantMap{{"active", true}, {"owned", true}};backend.m_googleSync["other-account"] = QVariantMap{{"active", true}, {"owned", true}};backend.m_account="fixture-account";backend.m_activeDraft="original-draft";
    require(backend.m_sendUndoMs==0,"normal Send has no artificial delay");backend.m_sendUndoMs=6000; // Retain delayed-queue safety regression.
    backend.m_enginePid=QCoreApplication::applicationPid();
    QJsonObject source=draft("fixture-account","original-draft"),other=draft("other-account","other-draft");
    backend.m_draft=source.toVariantMap();
    auto account=[](const QString &id){return QVariantMap{{"id",id},{"name",id},{"email",id+"@example.test"},
        {"lifecycle",QVariantMap{{"state","active"},{"revision",1}}},{"google",QVariantMap{{"capabilities",QVariantMap{{"send",true},{"mail_read",true}}}}}};};
    backend.m_allAccounts={account("fixture-account"),account("other-account")};
    require(backend.publishAccounts(backend.m_allAccounts),"active account list");
    QObject::disconnect(&backend.m_socket,SIGNAL(connected()),&backend,nullptr);
    backend.m_socket.connectToServer(fixture.server.serverName());
    await([&]{return backend.m_socket.state()==QLocalSocket::ConnectedState;},"cached socket");
    bool hello=false;backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto,bool ok){hello=ok;});await([&]{return hello;},"cached hello");
    QList<Held> sends,reads; QJsonArray retainedOutgoing; QJsonObject frozenOutgoingContent; bool holdRead=false;int saves=0; qint64 firstSendAt=-1;
    fixture.customRequest=[&](QLocalSocket *peer,const QJsonObject &request){
        const auto method=request["method"].toString(),id=request["id"].toString(),a=request["account"].toString();
        const auto params=request["params"].toObject();
        if(method=="google_sent") {peer->write(reply(id,{{"account",a},{"outgoing",retainedOutgoing},{"next_offset",QJsonValue::Null}}));return true;}
        if(method=="google_sent_message") {if(params["request_id"]==frozenOutgoingContent["request_id"]) peer->write(reply(id,frozenOutgoingContent)); else peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",id},{"ok",false},{"error",QJsonObject{{"code","not_found"},{"message","No retained fixture submission"}}}}).toJson(QJsonDocument::Compact)+'\n');return true;}
        if(method=="google_send") {if(firstSendAt<0)firstSendAt=backend.m_actionClock.elapsed();sends.append({peer,id,a,params});return true;}
        if(method=="draft_get" && holdRead) {reads.append({peer,id,a,params});return true;}
        auto &saved=a=="fixture-account"?source:other;
        if(method=="draft_get") peer->write(reply(id,saved));
        else if(method=="draft_save") {
            require(params["expected_revision"]==saved["revision"],"save original revision");
            for(const auto k:{"body","body_html","signature_text","subject","to_recipients","cc_recipients","bcc_recipients","attachment_ids"}) saved[k]=params[k];
            saved["revision"]=saved["revision"].toInteger()+1;++saves;peer->write(reply(id,saved));
        } else if(method=="accounts") peer->write(reply(id,{{"accounts",QJsonArray::fromVariantList(backend.m_allAccounts)}}));
        else if(method=="assist_settings_get") peer->write(reply(id,{{"revision",0},{"settings",QJsonObject{{"accounts",QJsonObject{}},{"model",QJsonValue::Null}}}}));
        else if(method=="view_get") peer->write(reply(id,{{"revision",1},{"view",QJsonObject{{"folder","drafts"},{"selectedDraft",saved["draft_id"]},{"replyOpen",true}}}}));
        else if(method=="view_save") peer->write(reply(id,{{"revision",params["expected_revision"].toInteger()+1}}));
        else peer->write(reply(id,{}));
        return true;
    };
    QTemporaryDir copy;const auto modulePath=copy.filePath("UndoTest");require(QDir().mkpath(modulePath),"module directory");
    QByteArray registration("module UndoTest\nsingleton Theme 1.0 Theme.qml\n");
    for(const auto &name:QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"},QDir::Files)) {
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name),"production QML copy");
        if(name=="ShortcutRegistry.qml") registration+="singleton ";
        if(name!="Theme.qml") registration+=name.chopped(4).toUtf8()+" 1.0 "+name.toUtf8()+'\n';
    }
    QFile module(modulePath+"/qmldir");require(module.open(QIODevice::WriteOnly),"module registration");
    module.write(registration);module.close();
    qmlRegisterType<ReplyDocument>("UndoTest",1,0,"ReplyDocument");
    QQmlApplicationEngine qml; FmIconItem::registerType();bool warnings=false;
    QObject::connect(&qml,&QQmlEngine::warnings,[&](const QList<QQmlError> &errors){warnings=true;for(const auto &error:errors)qWarning()<<error;});
    qml.addImportPath(copy.path());qml.rootContext()->setContextProperty("backend",&backend);
    qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import UndoTest 1.0
ApplicationWindow {width:1280;height:800;visible:true;color:Theme.bg
 property bool singleKeysAllowed:true;property bool searchPopupOpen:false
 function appearance(theme,size){Theme.themeId=theme;Theme.textSize=size}
 MailView {anchors.fill:parent}
 ActionToast {anchors.horizontalCenter:parent.horizontalCenter;anchors.bottom:parent.bottom;anchors.bottomMargin:16}
})QML",QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(),"production composer and toast load");
    auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());require(window,"window");
    await([&]{return window->isExposed();},"workspace exposed");
    auto mail=find(window->contentItem(),"mailWorkspace");require(mail,"production mail workspace");
    auto emptyRecipientDraft=source;emptyRecipientDraft["to_recipients"]=QJsonArray{};backend.acceptDraft(emptyRecipientDraft);
    require(!backend.googleSend()["can_send"].toBool(),"Send needs at least one recipient");
    backend.sendGoogleDraft();require(!backend.sendQueued() && sends.isEmpty() && saves==0,"empty routing never arms Send");
    backend.acceptDraft(source);await([&]{return mail->property("replyOpen").toBool();},"draft opens in workspace");
    auto click=[&](const QString &name){auto item=find(window->contentItem(),name);require(item && item->isEnabled(),"enabled control");require(QMetaObject::invokeMethod(item,"clicked"),"control click");};
    backend.stageDraftRecipient("bcc_recipients","Private <private@example.test>");
    backend.editDraft("Latest human text before Send");backend.m_draftTimer.stop();click("captureReply");
    await([&]{return backend.sendQueued();},"save then queue");
    await([&]{return !mail->property("replyOpen").toBool();},"Send closes real composer");
    require(!mail->property("collapsedDraftVisible").toBool(),"queued draft does not leave collapsed editor");
    require(backend.m_sendUndoMs==6000 && sends.isEmpty() && saves==1,"explicit delay begins after a real save");
    require(source["bcc_recipients"].toArray().size()==1 && source["body"]=="Latest human text before Send","staged routing and words saved");
    require(backend.draftLocked() && !backend.googleSend()["can_send"].toBool(),"queued draft protected");
    const auto originalFolder=backend.m_folder, originalSelection=backend.m_selected;
    const auto outgoing=backend.outgoingMessages(); require(outgoing.size()==1,"queued message has one preview");
    const auto pendingId=outgoing[0].toMap().value("id").toString();
    require(backend.conversations().isEmpty(),"pending send never enters Inbox");
    backend.m_folder="sent";backend.m_selected=pendingId;emit backend.queryChanged();backend.refreshOutgoingPresentation();
    require(backend.conversations().size()==1 && backend.conversation()["is_outgoing"].toBool(),"pending Sent row opens frozen reader");
    const auto pendingMessage=backend.conversation()["messages"].toList().first().toMap();
    require(pendingMessage["text"].toString()=="Latest human text before Send\n\nSignature" && pendingMessage["sent"].toBool() && pendingMessage["send_status"]=="sending","reader uses saved words/signature and sent semantics");
    require(backend.snapshotMailRows({pendingId}).size()==1,"pending row keyboard/context snapshot");
    backend.applyMailSelection(backend.snapshotMailRows({pendingId}),"trash");
    require(!backend.m_mutatingConversation && backend.m_bulkCommands.isEmpty(),"virtual identity never enters mail mutation");
    await([&]{return find(window->contentItem(),"messageBody_"+pendingId);},"pending message rendered");
    require(!mail->property("selectionMutable").toBool(),"pending selection disables regular mail commands");
    const auto captures=qEnvironmentVariable("FM_UNDO_SCREENSHOTS");
    if(!captures.isEmpty()) {
        QDir().mkpath(captures);
        for(const auto theme:{"graphite","paper","terminal"}) {
            QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString("default")));
            bool painted=false;const auto frame=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{painted=true;},Qt::QueuedConnection);
            window->requestUpdate();await([&]{return painted;},"theme paint completes");QObject::disconnect(frame);
            require(window->grabWindow().save(captures+"/send-undo-"+theme+".png"),"toast capture");
        }
        const auto originalToast=backend.m_actionToast;
        window->setWidth(480);
        QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString("paper")),Q_ARG(QVariant,QString("large")));
        backend.m_actionToast["text"]="Sending to Alex with a very long recipient name, and another recipient";
        emit backend.actionToastChanged();QCoreApplication::processEvents();
        auto toast=find(window->contentItem(),"actionToast"),undo=find(window->contentItem(),"actionToastButton");
        require(toast && undo && toast->width()<=336 && undo->width()>0 && undo->mapToScene(QPointF(undo->width(),0)).x()<=window->width(),"Undo remains visible with long text in narrow layout");
        require(window->grabWindow().save(captures+"/send-undo-paper-large.png"),"narrow large text capture");
        backend.m_actionToast=originalToast;emit backend.actionToastChanged();window->setWidth(1280);
    }
    const auto originalThread=backend.m_conversation;
    auto &queued=backend.m_queuedSends[backend.googleSendKey()];
    queued.preview["conversation_id"]="existing-thread";
    backend.m_selected="existing-thread";backend.m_folder="inbox";
    backend.m_conversation={{"id","existing-thread"},{"subject","An existing conversation"},{"revision",7},{"message_ids",QVariantList{"received"}},
        {"messages",QVariantList{QVariantMap{{"id","received"},{"sent_at","2026-09-15T10:00:00Z"},{"sender_name","Reader"},{"sender_email","reader@example.test"},{"text","Incoming words"},{"sent",false},{"unread",true}}}}};
    backend.refreshOutgoingPresentation();
    const auto bubbleMembers=backend.conversation()["messages"].toList();
    require(bubbleMembers.size()==2 && bubbleMembers.last().toMap()["id"]==pendingId && bubbleMembers.last().toMap()["sent"].toBool(),"reply bubble appended without treating outgoing as incoming");
    require(backend.m_conversation["messages"].toList().size()==1 && backend.m_conversation["message_ids"].toList()==QVariantList{"received"},"pending reply never changes canonical read-on-open or mutation targets");
    require(backend.snapshotMailRows({pendingId}).first().toMap()["is_outgoing"].toBool(),"reply bubble menu names this exact outgoing request");
    queued.preview["conversation_id"]="";backend.m_conversation=originalThread;
    backend.m_folder=originalFolder;backend.m_selected=originalSelection;emit backend.queryChanged();backend.refreshOutgoingPresentation();
    click("actionToastButton");await([&]{return !backend.sendQueued() && backend.m_inFlight.isEmpty();},"Undo keeps original draft");
    await([&]{return mail->property("replyOpen").toBool();},"Undo reopens real composer");
    require(backend.outgoingMessages().isEmpty(),"Undo removes the exact pending Sent row and bubble");
    require(sends.isEmpty() && backend.m_draft["body"]==source["body"].toVariant() && !backend.draftLocked(),"Undo makes no submission and restores exact words");
    const auto savedEditor=backend.m_draft;const auto savedActive=backend.m_activeDraft;
    auto durable=outgoing[0].toMap();durable["state"]="accepted";durable["source_version"]=QString(64,'a');durable.remove("text");
    frozenOutgoingContent=QJsonObject::fromVariantMap(durable);frozenOutgoingContent["text"]="Durable frozen words";frozenOutgoingContent["text_truncated"]=false;frozenOutgoingContent["attachments"]=QJsonArray{};
    retainedOutgoing={QJsonObject::fromVariantMap(durable)};
    backend.m_selected=pendingId;backend.m_activeDraft.clear();backend.m_draft.clear();backend.remember("replyOpen",false);
    backend.loadOutgoing();await([&]{return !backend.m_outgoingBodies.isEmpty() && backend.m_inFlight.isEmpty();},"persisted outgoing query and worker content return");
    require(backend.conversation()["messages"].toList().first().toMap()["text"]=="Durable frozen words","reader uses retained submitted MIME after in-memory queue is gone");
    backend.m_draft["body"]="A later editor must not change sent text";
    require(backend.conversation()["messages"].toList().first().toMap()["text"]=="Durable frozen words","later draft does not leak into sent reader");
    retainedOutgoing={};frozenOutgoingContent["observed_conversation_id"]="acquired-thread";
    backend.loadOutgoing();await([&]{return backend.m_selected=="acquired-thread" && backend.m_inFlight.isEmpty();},"acquisition replaces selected temporary identity through a read-only open");
    require(backend.outgoingMessages().isEmpty(),"acquisition removes duplicate Sent preview");
    backend.m_selected=originalSelection;backend.m_activeDraft=savedActive;backend.m_conversation=originalThread;backend.m_outgoingBodies.clear();
    backend.acceptDraft(QJsonObject::fromVariantMap(savedEditor));backend.remember("replyOpen",true);emit backend.presentationChanged();backend.refreshOutgoingPresentation();
    await([&]{return mail->property("replyOpen").toBool();},"original draft remains available after read-only Sent browsing");
    click("captureReply");require(backend.sendQueued(),"second explicit Send queues");
    const auto deadline=backend.m_queuedSends[backend.googleSendKey()].deadline;
    QElapsedTimer elapsed;elapsed.start();
    auto google=source["google"].toObject();google["revision"]=9;google["state"]="synced";source["google"]=google;
    backend.selectAccount("other-account");await([&]{return backend.m_activeDraft=="other-draft";},"browse another real saved view during interval");
    backend.editDraft("New words in another mailbox");backend.m_draftTimer.stop();
    while(elapsed.elapsed()<6500 && sends.isEmpty())QCoreApplication::processEvents(QEventLoop::AllEvents,10);
    require(sends.size()==1 && firstSendAt>=deadline,"one request only after real six-second window");
    const auto sent=sends[0];require(sent.account=="fixture-account" && sent.params["draft_id"]=="original-draft" && sent.params["expected_revision"]==source["revision"] && sent.params["expected_google_revision"]==9,"deadline read follows original draft and current Google revision");
    require(backend.m_draft["body"]=="New words in another mailbox" && backend.m_draftDirty,"navigation never replaces another editor");
    click("captureReply");await([&]{return backend.sendQueued();},"another mailbox gets its own Undo");
    const auto latestToast=backend.m_actionToast;
    QJsonObject accepted{{"account",sent.account},{"draft_id",sent.params["draft_id"]},{"request_id",sent.params["request_id"]},
        {"request",sent.params},{"revision",3},{"current_google_revision",12},{"google_revision",9},{"draft_revision",sent.params["expected_revision"]},
        {"state","accepted"},{"operation_is_current",true},{"last_accepted_draft_revision",sent.params["expected_revision"]}};
    sent.peer->write(reply(sent.id,accepted));await([&]{return backend.m_googleOperations.isEmpty();},"accepted receipt completes");
    require(backend.m_actionToast==latestToast,"older success cannot remove newer Undo");
    click("actionToastButton");await([&]{return !backend.sendQueued() && backend.m_inFlight.isEmpty();},"second Undo");
    backend.advanceSendUndo();require(backend.m_deferredActionToasts.isEmpty() && backend.actionToast()["text"].toString()=="Sent to Reader","previous accepted result follows newer Undo");
    backend.m_actionToastDeadline=backend.m_actionClock.elapsed();backend.advanceSendUndo();
    require(!backend.m_actionTimer.isActive() && !backend.actionToast()["visible"].toBool(),"idle toast stops its timer");
    backend.m_googleSendUi.clear();backend.m_draftDirty=false;backend.acceptDraft(other);click("captureReply");require(backend.sendQueued(),"queue before held deadline read");
    holdRead=true;backend.m_queuedSends[backend.googleSendKey()].deadline=backend.m_actionClock.elapsed();backend.advanceSendUndo();
    await([&]{return reads.size()==1;},"deadline read held");backend.cancelGoogleSend();
    holdRead=false;reads[0].peer->write(reply(reads[0].id,other));await([&]{return backend.m_inFlight.isEmpty();},"late read drains");
    require(sends.size()==1 && !backend.sendQueued(),"late read cannot rearm cancellation");
    backend.m_draftDirty=false;backend.acceptDraft(other);click("captureReply");
    other["revision"]=other["revision"].toInteger()+1;
    backend.m_queuedSends[backend.googleSendKey()].deadline=backend.m_actionClock.elapsed();backend.advanceSendUndo();
    await([&]{return !backend.sendQueued();},"another writer's edit stops pending Send");require(sends.size()==1,"changed authored revision never submitted");
    backend.acceptDraft(other);click("captureReply");require(backend.sendQueued(),"queue before account lifecycle change");
    auto changed=backend.m_allAccounts[1].toMap();changed["lifecycle"]=QVariantMap{{"state","active"},{"revision",2}};backend.m_allAccounts[1]=changed;
    backend.advanceSendUndo();require(!backend.sendQueued() && sends.size()==1,"remove-restore lifecycle cancels before submission");
    backend.acceptDraft(other);click("captureReply");require(backend.sendQueued(),"queue before connection loss");
    backend.m_socket.abort();await([&]{return backend.m_queuedSends.isEmpty();},"connection loss cancels queued send");
    require(sends.size()==1 && !backend.m_actionTimer.isActive(),"disconnect starts no Send and stops timer");
    backend.m_connected=true;backend.m_googleSync["other-account"] = QVariantMap{{"active",true},{"owned",true}};backend.m_socket.connectToServer(fixture.server.serverName());
    await([&]{return backend.m_socket.state()==QLocalSocket::ConnectedState;},"reconnect fixture");
    hello=false;backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto,bool ok){hello=ok;});await([&]{return hello;},"reconnected cached hello");
    // Normal Send dispatches as soon as local preflight returns, with persistent progress.
    backend.m_sendUndoMs=0;backend.acceptDraft(other);const auto fastStarted=backend.m_actionClock.elapsed();click("captureReply");
    await([&]{return sends.size()==2;},"normal Send reaches provider without Undo delay");
    require(backend.m_actionClock.elapsed()-fastStarted<1500 && backend.actionToast()["progress"].toBool(),"fast send dispatch and visible indeterminate progress");
    require(backend.googleSend()["status"].toString()=="Sending…","normal sending has one clear status without obsolete Undo instructions");
    backend.m_actionToastDeadline=backend.m_actionClock.elapsed();backend.advanceSendUndo();
    require(backend.actionToast()["visible"].toBool(),"slow upload retains progress beyond toast expiry");
    auto fast=sends.last();auto fastAccepted=accepted;fastAccepted["account"]=fast.account;fastAccepted["draft_id"]=fast.params["draft_id"];fastAccepted["request_id"]=fast.params["request_id"];fastAccepted["request"]=fast.params;fastAccepted["draft_revision"]=fast.params["expected_revision"];fastAccepted["google_revision"]=fast.params["expected_google_revision"];fastAccepted["last_accepted_draft_revision"]=fast.params["expected_revision"];
    fast.peer->write(reply(fast.id,fastAccepted));await([&]{return backend.m_googleOperations.isEmpty();},"immediate send accepted");
    require(backend.actionToast()["success"].toBool() && !backend.actionToast()["progress"].toBool(),"progress becomes confirmed Sent feedback");
    require(!mail->property("replyOpen").toBool() && !mail->property("collapsedDraftVisible").toBool(),"accepted unchanged composer fully closes");
    backend.m_draftDirty=true;require(!backend.googleSend()["sent_current"].toBool(),"newer unsaved text remains editable");backend.m_draftDirty=false;
    backend.m_sendUndoMs=6000;other["revision"]=other["revision"].toInteger()+1;
    backend.acceptDraft(other);click("captureReply");require(backend.sendQueued(),"queue before close");
    backend.shutdown();require(backend.m_queuedSends.isEmpty() && sends.size()==2,"GUI close cancels unsubmitted work");
    require(!warnings,"production QML has no warnings");
    qInfo()<<"PASS: real six-second Undo, save/routing, navigation, current Google revision, toast ownership, cancellation, changed draft and close";
}
};
int main(int argc,char **argv){QGuiApplication app(argc,argv);QGuiApplication::setFont(QFont("Geist",10));QQuickStyle::setStyle("Basic");SendUndoTest::run();return 0;}
