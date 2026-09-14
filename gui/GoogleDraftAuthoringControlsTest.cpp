#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QImage>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
using namespace GoogleOperationFixture;
namespace {
QQuickItem *named(QQuickItem *item,const QString &name) {
    if(item->objectName()==name)return item;
    for(auto child:item->childItems())if(auto found=named(child,name))return found;
    return nullptr;
}
QJsonObject snapshot(int revision=3) {
    return {{"account","fixture-account"},{"draft_id","remote-draft"},{"origin","google_snapshot"},{"read_only",true},
        {"revision",1},{"mode","new"},{"subject","Exact remote subject 日本語"},{"body","Retained source body"},
        {"body_html",QJsonValue::Null},{"from","alias@example.test"},{"from_name","Original author"},{"signature_text",""},
        {"to_recipients",QJsonArray{QJsonObject{{"name","Recipient"},{"email","to@example.test"}}}},
        {"cc_recipients",QJsonArray{}},{"bcc_recipients",QJsonArray{}},{"attachment_ids",QJsonArray{}},{"attachments",QJsonArray{}},
        {"google",QJsonValue::Null},{"google_send",QJsonValue::Null},{"google_delete",QJsonValue::Null},
        {"remote_source",QJsonObject{{"provider_draft_id","ra0000"},{"message_id","rm0000"},{"thread_id","rt0000"},
            {"source_revision",revision},{"sha256",QString(64,'a')},{"size",321},{"source_state","available"},{"preview_state","complete"},
            {"linked_local_draft_id","existing-local"},{"raw_headers",QJsonObject{{"from","Original author <alias@example.test>"}}}}}};
}
QJsonObject assessment(int revision=3) {
    return {{"account","fixture-account"},{"draft_id","remote-draft"},{"source_revision",revision},{"source_sha256",QString(64,'a')},
        {"provider_draft_id","ra0000"},{"primary_from",QJsonObject{{"name","Primary"},{"email","primary@example.test"}}},
        {"copy",QJsonObject{{"allowed",true},{"reasons",QJsonArray{}},{"losses",QJsonArray{"primary_from","reply_thread","separate_autosync"}}}},
        {"adopt",QJsonObject{{"allowed",true},{"reasons",QJsonArray{}},{"requires_remote_check",true}}},
        {"delete",QJsonObject{{"allowed",true},{"reasons",QJsonArray{}},{"requires_remote_check",true}}}};
}
QJsonObject localResult(const QString &action,const QString &id="new-local") {
    auto result=snapshot();result["draft_id"]=id;result["origin"]="local";result["read_only"]=false;result.remove("remote_source");
    result["google"]=QJsonObject{{"account","fixture-account"},{"draft_id",id},{"revision",1},{"local_revision",1},
        {"state",action=="adopt"?"adopted":"local"},{"operation",QJsonValue::Null},{"baseline",QJsonValue::Null},
        {action=="copy"?"derivation":"adoption",QJsonObject{{"source_draft_id","remote-draft"},{"source_revision",3}}}};
    return result;
}
QJsonObject deletion(const QString &request,const QString &state,int revision=1) {
    return {{"account","fixture-account"},{"draft_id","remote-draft"},{"related_draft_ids",QJsonArray{"remote-draft","existing-local"}},
        {"request_id",request},{"revision",revision},{"current_google_revision",revision},{"operation_is_current",true},
        {"source_revision",3},{"source_sha256",QString(64,'a')},{"provider_draft_id","ra0000"},{"state",state}};
}
QByteArray failure(const QString &id,const QString &code,const QJsonObject &details={}) {
    return QJsonDocument(QJsonObject{{"version",1},{"id",id},{"ok",false},
        {"error",QJsonObject{{"code",code},{"message","Expected definitive fixture refusal"},{"details",details}}}}).toJson(QJsonDocument::Compact)+'\n';
}
}
class GoogleDraftAuthoringControlsTest {
public:
static void run() {
    Engine fixture;MailBackend backend(fixture.profile,"unused",nullptr,false,true);
    backend.m_connected=true;backend.m_account="fixture-account";backend.m_activeDraft="remote-draft";
    backend.m_generation=1;backend.m_enginePid=QCoreApplication::applicationPid();backend.m_draft=snapshot().toVariantMap();
    backend.m_accounts={QVariantMap{{"id","fixture-account"},{"google",QVariantMap{{"capabilities",QVariantMap{{"mail_read",true},{"mail_delivery",true},{"send",true}}}}}}};
    backend.m_googleSync={{"fixture-account",QVariantMap{{"owned",true},{"active",true}}}};
    QObject::disconnect(&backend.m_socket,SIGNAL(connected()),&backend,nullptr);
    backend.m_socket.connectToServer(fixture.server.serverName());
    await([&]{return backend.m_socket.state()==QLocalSocket::ConnectedState;},"cached socket");
    bool hello=false;backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto,bool ok){hello=ok;});await([&]{return hello;},"cached hello");
    QPointer<QLocalSocket> held;QString heldId;QJsonObject heldParams;QString heldMethod;
    int actions=0,assessments=0;bool holdAssessment=false;QString statusMode="unknown";
    fixture.customRequest=[&](QLocalSocket *peer,const QJsonObject &request) {
        const auto method=request["method"].toString(),id=request["id"].toString();const auto params=request["params"].toObject();
        if(method=="google_draft_authoring") {
            ++assessments;if(!holdAssessment){peer->write(reply(id,assessment(params["source_revision"].toInt())));return true;}
        } else if(method=="google_draft_delete_status") {
            if(statusMode=="missing")peer->write(failure(id,"not_found"));
            else peer->write(reply(id,deletion(params["request_id"].toString(),statusMode=="prepared"?"prepared":"acceptance_unknown")));
            return true;
        } else if(method=="google_draft_delete_reconcile") {
            auto result=deletion(params["request_id"].toString(),"acceptance_unknown",2);
            result["read_evidence"]=QJsonObject{{"draft_exists",false},{"deletion_proven",false},{"source","draft_get"},{"observed",QJsonValue::Null}};
            peer->write(reply(id,result));return true;
        } else if(method=="google_draft_delete_abandon") {
            peer->write(reply(id,deletion(params["request_id"].toString(),"abandoned",params["expected_revision"].toInt()+1)));return true;
        } else if(method!="google_draft_copy"&&method!="google_draft_adopt"&&method!="google_draft_delete")return false;
        else ++actions;
        held=peer;heldId=id;heldParams=params;heldMethod=method;return true;
    };
    QTemporaryDir copy;const auto modulePath=copy.filePath("DraftTest");require(QDir().mkpath(modulePath),"QML module");
    for(const auto name:{"Theme.qml","FmButton.qml","FmToolTip.qml","FmIcon.qml","RecipientField.qml","ReplyComposer.qml"})
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name),"copy production QML");
    QFile module(modulePath+"/qmldir");require(module.open(QIODevice::WriteOnly),"QML metadata");
    module.write("module DraftTest\nsingleton Theme 1.0 Theme.qml\nReplyComposer 1.0 ReplyComposer.qml\nFmButton 1.0 FmButton.qml\nFmToolTip 1.0 FmToolTip.qml\nFmIcon 1.0 FmIcon.qml\nRecipientField 1.0 RecipientField.qml\n");module.close();
    for(const auto font:{"Geist-Regular.ttf","Geist-Medium.ttf","Geist-SemiBold.ttf"})
        require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR)+"/fonts/"+font)>=0,"bundled fonts");
    qmlRegisterType<ReplyDocument>("DraftTest",1,0,"ReplyDocument");QQmlApplicationEngine qml;bool warnings=false;
    QObject::connect(&qml,&QQmlEngine::warnings,[&](const QList<QQmlError>&errors){warnings=true;for(const auto &e:errors)qWarning()<<e;});
    qml.addImportPath(copy.path());qml.rootContext()->setContextProperty("backend",&backend);
    qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import DraftTest 1.0
ApplicationWindow { width:390; height:1500; visible:true; color:Theme.reader
    palette.text:Theme.text; palette.base:Theme.surface; palette.window:Theme.canvas
    palette.button:Theme.surface; palette.buttonText:Theme.text; palette.highlight:Theme.focusBorder
    palette.highlightedText:Theme.text; palette.mid:Theme.scrollPalette; palette.dark:Theme.scrollPalette
    font.family:Theme.fontFamily; font.pixelSize:Theme.fontSize(14)
    function appearance(theme,size) { Theme.themeId=theme;Theme.textSize=size }
    ReplyComposer { objectName:"composer";width:parent.width }
})QML",QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(),"production action QML loads");auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());require(window,"window");
    auto activate=[&](const QString &name){auto control=named(window->contentItem(),name);require(control&&control->isVisible()&&control->isEnabled(),qPrintable("active control "+name));require(QMetaObject::invokeMethod(control,"clicked"),"actual QML handler");};
    auto reset=[&]{backend.m_account="fixture-account";backend.m_activeDraft="remote-draft";backend.m_draft=snapshot().toVariantMap();++backend.m_generation;backend.m_draftSourceActions.clear();backend.m_draftDeletionSelected={};emit backend.accountChanged();emit backend.draftChanged();};
    auto load=[&]{activate("draftSourceActionsButton");await([&]{return !backend.m_draftAuthoringLoading;},"exact assessment");};
    auto confirm=[&](const QString &action){heldId.clear();activate(action+"GoogleDraftSource");activate("confirmDraftSourceAction");await([&]{return !heldId.isEmpty();},"actual action request");};
    require(actions==0&&assessments==0,"view does not prepare or upload a draft");
    for(const auto theme:{"graphite","paper","harbour"})for(const auto size:{"default","large"}) {
        require(QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(size))),"theme");load();
        for(const auto name:{"copyGoogleDraftSource","adoptGoogleDraftSource","deleteGoogleDraftSource"}) {
            auto control=named(window->contentItem(),name);QCoreApplication::processEvents();
            const auto r=control->mapRectToScene(QRectF(0,0,control->width(),control->height()));
            require(r.width()>100&&QRectF(0,0,window->width(),window->height()).contains(r),"action popup contained at 390px");
        }
        const auto captures=qEnvironmentVariable("FM_GUI_ACTION_SCREENSHOTS");
        if(!captures.isEmpty()){require(QDir().mkpath(captures),"captures path");const auto name=QDir(captures).filePath(QString("actions-%1-%2.png").arg(theme,size));require(!QFileInfo::exists(name)&&window->grabWindow().save(name),"retain open actions screenshot");}
        activate("copyGoogleDraftSource");auto disclosure=named(window->contentItem(),"draftSourceActionDisclosure");
        require(disclosure->property("text").toString().contains("primary@example.test")&&disclosure->property("text").toString().contains("signature"),"explicit primary From and copy losses disclosed");
        require(named(window->contentItem(),"draftSourceActionTitle")->property("color")==disclosure->property("color"),"confirmation title uses the same readable theme ink as its disclosure");
        QCoreApplication::processEvents();const auto path=qEnvironmentVariable("FM_GUI_ACTION_SCREENSHOTS");
        if(!path.isEmpty()){require(QDir().mkpath(path),"captures path");const auto name=QDir(path).filePath(QString("copy-%1-%2.png").arg(theme,size));require(!QFileInfo::exists(name)&&window->grabWindow().save(name),"retain confirmation screenshot");}
        activate("cancelDraftSourceAction");
    }
    require(actions==0,"opening/canceling confirmations performs no action");
    load();auto stale=backend.draftSourceActionContext("copy");backend.m_draft=snapshot(4).toVariantMap();emit backend.draftChanged();backend.confirmDraftSourceAction(stale);
    require(actions==0,"changed exact source invalidates confirmation");reset();
    holdAssessment=true;heldId.clear();activate("draftSourceActionsButton");await([&]{return !heldId.isEmpty();},"held full-source assessment");
    backend.m_draft=snapshot(4).toVariantMap();emit backend.draftChanged();backend.m_draft=snapshot().toVariantMap();emit backend.draftChanged();held->write(reply(heldId,assessment()));
    await([&]{return backend.m_googleOperations.isEmpty();},"old ABA assessment delivered");require(backend.m_draftAuthoring.isEmpty(),"old assessment cannot restore action eligibility after ABA");holdAssessment=false;reset();
    load();confirm("copy");const auto original=heldParams;
    require(original["from_policy"]=="primary"&&original["content_policy"]=="plain_text"&&original["inline_policy"]=="attachments"&&original["thread_policy"]=="omit","all explicit copy policies frozen");
    ++backend.m_generation;backend.m_draft=snapshot(4).toVariantMap();emit backend.draftChanged();const auto changed=backend.m_draft;
    held->write(reply(heldId,localResult("copy")));await([&]{return backend.m_googleOperations.isEmpty();},"late copy commit returns");
    require(backend.m_draft==changed&&backend.m_activeDraft=="remote-draft"&&backend.draftAuthoring().value("result_draft_id")=="new-local","late result retained without navigation or text replacement");
    reset();load();confirm("copy");held->write(failure(heldId,"stale_source_revision",{{"draft_authoring_invocation_finished_without_commit",true}}));
    await([&]{return backend.m_googleOperations.isEmpty();},"fresh definitive copy refusal");require(!backend.draftAuthoring().value("retry").toBool(),"fresh no-commit refusal permits reassessment");
    load();confirm("copy");const auto uncertain=heldParams;held->write(failure(heldId,"transport_error"));await([&]{return backend.m_googleOperations.isEmpty();},"ambiguous copy reply");
    require(backend.draftAuthoring().value("retry").toBool(),"ambiguous copy retains explicit retry");heldId.clear();activate("retryDraftSourceAction");await([&]{return !heldId.isEmpty();},"same action replay");
    require(heldParams==uncertain,"retry retains exact request identity/policies");held->write(failure(heldId,"stale_source_revision",{{"draft_authoring_invocation_finished_without_commit",true}}));
    await([&]{return backend.m_googleOperations.isEmpty();},"later invocation proof");require(backend.draftAuthoring().value("retry").toBool(),"invocation proof cannot erase prior uncertainty");
    reset();load();confirm("adopt");++backend.m_generation;auto existing=localResult("adopt","existing-local");auto g=existing["google"].toObject();g.remove("adoption");g["state"]="synced";
    g["effective_target"]=QJsonObject{{"provider_draft_id","ra0000"},{"message_id","rm0000"},{"sha256",QString(64,'a')},{"size",321}};existing["google"]=g;
    held->write(reply(heldId,existing));await([&]{return backend.m_googleOperations.isEmpty();},"compatible existing owner returns");
    require(!backend.draftAuthoring().value("retry").toBool()&&backend.draftAuthoring().value("result_draft_id")=="existing-local","compatible linked baseline owner accepted without fabricated adoption");
    reset();load();confirm("delete");statusMode="missing";held->write(failure(heldId,"stale_source_revision",{{"draft_delete_invocation_finished_before_dispatch",true}}));
    await([&]{return backend.m_googleOperations.isEmpty()&&!backend.m_draftDeletionLoading&&backend.m_pending.isEmpty();},"fresh delete refusal then exact absent receipt");
    require(!backend.m_draftSourceActions.value("fixture-account\nremote-draft").uncertain,"fresh completed predispatch refusal plus absent exact journal permits new confirmation");
    load();confirm("delete");const auto request=heldParams["request_id"].toString();held->write(failure(heldId,"transport_error"));
    await([&]{return backend.m_googleOperations.isEmpty()&&!backend.m_draftDeletionLoading&&backend.m_pending.isEmpty();},"ambiguous deletion plus not_found");
    require(backend.m_draftSourceActions.value("fixture-account\nremote-draft").uncertain,"untyped not_found never releases uncertain deletion");
    statusMode="unknown";activate("readDraftDeletion");await([&]{return !backend.m_draftDeletionLoading;},"read exact deletion receipt");
    activate("checkDraftDeletion");await([&]{return !backend.m_draftDeletionActive;},"read-only Google reconciliation");
    require(backend.draftDeletion().value("receipt").toMap().value("state")=="acceptance_unknown"
        &&backend.draftDeletion().value("status").toString().contains("does not establish"),"absence evidence never becomes deletion acceptance");
    const auto before=backend.m_draft;auto old=deletion(request,"prepared",1);require(!backend.mergeDraftDeletion(old,"fixture-account","remote-draft")&&backend.m_draft==before,"older receipt cannot replace unknown newer revision");
    backend.m_draftDeletionSelected={};backend.m_draft["google_delete"]=QVariant{};statusMode="prepared";activate("readDraftDeletion");await([&]{return !backend.m_draftDeletionLoading;},"prepared receipt readable");
    activate("abandonDraftDeletion");activate("confirmAbandonDraftDeletion");await([&]{return !backend.m_draftDeletionActive;},"explicit revision-bound local abandonment");
    require(backend.draftDeletion().value("receipt").toMap().value("state")=="abandoned","prepared deletion abandoned without provider write");
    require(!warnings,"no QML warnings in action controls");
    backend.m_connected=false;backend.m_socket.abort();qInfo()<<"Native source actions, identity races, proof recovery and deletion evidence passed.";
}
};
int main(int argc,char **argv){QQuickStyle::setStyle("Basic");QGuiApplication app(argc,argv);GoogleDraftAuthoringControlsTest::run();return 0;}
