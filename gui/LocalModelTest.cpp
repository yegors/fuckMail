#include "FmIconItem.h"
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QQuickItem>
#include <QQuickStyle>
#include <QTemporaryDir>
#include <QDir>
#include <QFile>
#include <QTimer>
#include <QElapsedTimer>
#include <QThread>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QJsonDocument>
#include <QJSValue>
#include <QDebug>
#include <QFontDatabase>
#include <functional>
#include <algorithm>
#include <vector>

static void require(bool ok,const char *message){if(!ok)qFatal("%s",message);}
template<typename F>static void await(F predicate){QElapsedTimer timer;timer.start();while(!predicate() && timer.elapsed()<5000){QCoreApplication::processEvents();QThread::msleep(5);}require(predicate(),"Native local model wait timed out");}
static QQuickItem *find(QQuickItem *root,const QString &name){if(root->objectName()==name)return root;for(auto child:root->childItems())if(auto match=find(child,name))return match;return nullptr;}
class Backend : public QObject {
 Q_OBJECT
 Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
 Q_PROPERTY(QString account READ account CONSTANT)
 Q_PROPERTY(QString selectedId READ selectedId NOTIFY selectionChanged)
 Q_PROPERTY(QVariantList accounts READ accounts CONSTANT)
 Q_PROPERTY(QVariantList categories READ categories CONSTANT)
public:
 bool online=true; QString selected="conversation";
 bool connected()const{return online;}QString account()const{return "personal";}
 QString selectedId()const{return selected;}
 QVariantList accounts()const{return {QVariantMap{{"id","personal"},{"name","Personal"}}};}
 QVariantList categories()const{return {QVariantMap{{"id","work"},{"name","Work"}}};}
 QVariantMap settings{{"endpoint","http://127.0.0.1:11434"},{"model",QVariant()},{"prompt","Summarize faithfully. {{labels}} {{messages}} {{partial}}"},{"accounts",QVariantMap{}}};
 int sequence=0,revision=0,inferences=0,cancellations=0,watches=0;bool reject=false,delayStart=false,delayPoll=false,failPoll=false;QVariantMap lastSave;
 std::vector<std::function<void()>> starts,polls,discoveries,results;
 bool delayResults=false; QVariantMap completedItems;
 bool delayDiscovery=false;
 QVariantMap lastDiscovery;
 QVariantMap targets;
 QString summaryText="Alex approved the revised design. Meet on Friday at 10 to discuss implementation; the time is still a proposal.";
 void flush(std::vector<std::function<void()>> &queue){auto pending=std::move(queue);queue.clear();for(auto &reply:pending)reply();}
 QVariantMap summary(QString target={})const{return {{"task","summary"},{"target",target},{"output",QVariantMap{{"summary",summaryText},{"source_ids",QVariantList{"message"}}}},{"source_ids",QVariantList{"message"}},{"stale",false},{"partial",false},{"model",QVariantMap{{"name","gemma3n:e4b"}}},{"created_at","2026-09-14T22:00:00Z"}};}

 Q_INVOKABLE QString assistanceRequest(QString method,QVariantMap params,QString = {}){
  const auto ticket=QString::number(++sequence);
  QTimer::singleShot(0,this,[this,ticket,method,params]{
   QVariantMap result;bool ok=true;
   if(method=="assist_settings_get")result={{"revision",revision},{"settings",settings},{"defaults",settings}};
   else if(method=="assist_discover"){
    lastDiscovery=params;
    if(params["provider"]=="openai_compatible")result={{"models",QVariantList{QVariantMap{{"name","server-model"},{"digest",""},{"eligible",true}}}}};
    else result={{"version","fixture"},{"models",QVariantList{QVariantMap{{"name","local:latest"},{"digest",QString(64,'a')},{"eligible",true}},QVariantMap{{"name","remote:cloud"},{"digest",QString(64,'b')},{"eligible",false},{"reason","Cloud model"}}}}};
    if(delayDiscovery){discoveries.push_back([this,ticket,method,result]{emit assistanceReply(ticket,method,result,true);});return;}
   }
   else if(method=="assist_watch"){++watches;result={{"watching",false}};}
   else if(method=="assist_catalog")result={{"categories",categories()}};
   else if(method=="assist_results") {
    QVariantList items{summary()};for(auto it=completedItems.cbegin();it!=completedItems.cend();++it)items.append(it.value());
    result={{"items",selected=="conversation" ? items : QVariantList{}}};
    if(delayResults){results.push_back([this,ticket,method,result]{emit assistanceReply(ticket,method,result,true);});return;}
   }
   else if(method=="assist_settings_save"){
    if(reject){ok=false;result={{"message","Settings changed; reload"}};}
    else{lastSave=params;settings=params["settings"].toMap();++revision;result={{"revision",revision},{"settings",settings},{"defaults",settings}};}
   }else if(method=="assist_start"){
    ++inferences;auto id="job-"+QString::number(inferences);targets[id]=params;result={{"job_id",id},{"status","queued"}};
    if(delayStart){starts.push_back([this,ticket,method,result]{emit assistanceReply(ticket,method,result,true);});return;}
   }else if(method=="assist_cancel"){++cancellations;result={{"status","canceled"}};}
   else if(method=="assist_job"){
    if(failPoll){ok=false;result={{"message","The local model disconnected. Try again."}};}
    else {const auto target=targets[params["job_id"].toString()].toMap()["message_id"].toString();if(!target.isEmpty())completedItems[target]=summary(target);result={{"status","completed"}};}
    if(delayPoll){polls.push_back([this,ticket,method,result,ok]{emit assistanceReply(ticket,method,result,ok);});return;}
   }
   emit assistanceReply(ticket,method,result,ok);
  });return ticket;
 }
 Q_INVOKABLE void refresh(){}
 Q_INVOKABLE QString fixedFontFamily() const { return QFontDatabase::systemFont(QFontDatabase::FixedFont).family(); }
signals:
 void assistanceReply(QString ticket,QString method,QVariantMap result,bool ok);
 void connectedChanged();void conversationChanged();void selectionChanged();
};
int main(int argc,char **argv){
 QGuiApplication app(argc,argv);QQuickStyle::setStyle("Basic");
 for(const auto file:{"Geist-Regular.ttf","Geist-Medium.ttf","Geist-SemiBold.ttf"})require(QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR)+"/fonts/"+file)>=0,"load shipped font");
 QGuiApplication::setFont(QFont("Geist",10));
 Backend backend;QFile promptFile(QString(FM_GUI_SOURCE_DIR)+"/../crates/fm-assist/src/message-request.json");require(promptFile.open(QIODevice::ReadOnly),"default prompt available");backend.settings["prompt"]=QString::fromUtf8(promptFile.readAll());QTemporaryDir temp;auto dir=temp.filePath("ModelTest");require(QDir().mkpath(dir),"module directory");
 for(const auto name:{"FmScrollBar.qml","Theme.qml","FmCheckBox.qml","FmButton.qml","FmToolTip.qml","FmIcon.qml","FmComboBox.qml","FmTextField.qml","LocalModelSettings.qml","MailAssistance.qml","SummaryCard.qml","MessageAssistance.qml","MessageHeaderButton.qml","SummarizeAction.qml"})require(QFile::copy(QString(FM_GUI_SOURCE_DIR)+"/"+name,dir+"/"+name),"copy production QML");
 QFile module(dir+"/qmldir");require(module.open(QIODevice::WriteOnly),"module metadata");module.write("module ModelTest\nsingleton Theme 1.0 Theme.qml\nLocalModelSettings 1.0 LocalModelSettings.qml\nMailAssistance 1.0 MailAssistance.qml\nSummaryCard 1.0 SummaryCard.qml\nMessageAssistance 1.0 MessageAssistance.qml\nMessageHeaderButton 1.0 MessageHeaderButton.qml\nSummarizeAction 1.0 SummarizeAction.qml\n");module.close();
 QQmlApplicationEngine qml; FmIconItem::registerType();bool warnings=false;QObject::connect(&qml,&QQmlEngine::warnings,[&](auto errors){warnings=true;for(const auto &e:errors)qWarning()<<e;});qml.addImportPath(temp.path());qml.rootContext()->setContextProperty("backend",&backend);
 qml.loadData(R"(import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import ModelTest 1.0
ApplicationWindow {width:680;height:900;visible:true;color:Theme.canvas
 function appearance(theme,size){Theme.themeId=theme;Theme.textSize=size}
 ScrollView{id:settingsScroll;anchors.fill:parent;LocalModelSettings{id:panel;objectName:"panel";width:620}}
 property string openedSource: ""
 function showReader(){settingsScroll.visible=false;reader.active=true}
 Loader{id:reader;anchors.fill:parent;active:false;sourceComponent:Component{Item{
 MailAssistance{id:controller;objectName:"controller";onOpenSource:id=>openedSource=id}
 ColumnLayout{anchors{left:parent.left;right:parent.right;top:parent.top;margins:40} spacing:22
  Text{text:"Design review";color:Theme.text;font.family:Theme.fontFamily;font.pixelSize:Theme.fontSize(28);font.weight:Font.Medium}
  Text{text:"Alex · 2 messages · 2 h ago · Personal";color:Theme.muted;font.family:Theme.fontFamily;font.pixelSize:Theme.fontSize(13)}
  SummaryCard{objectName:"summaryFixture";Layout.fillWidth:true;result:controller.resultFor("summary","");busy:controller.workingOn("summary","");error:controller.errorFor("summary","");canSummarize:controller.summariesEnabled && !controller.busy;members:[{id:"message",sender_name:"Alex",sent_at:"2026-09-14T22:00:00Z"}];onRegenerate:controller.start("summary","");onCancelRequested:controller.cancel();onOpenSource:id=>openedSource=id}
  RowLayout{Layout.fillWidth:true
   Text{text:"Alex";Layout.fillWidth:true;color:Theme.text;font.family:Theme.fontFamily;font.pixelSize:Theme.fontSize(13);font.weight:Font.Medium}
   SummarizeAction{objectName:"summarizeMessage_"+receivedActions.message.id;visible:receivedActions.showSummarize;enabled:receivedActions.canSummarize;modelName:receivedActions.modelName;onClicked:receivedActions.summarize()}
  }
  MessageAssistance{id:receivedActions;objectName:"receivedActions";showKindControl:false;Layout.fillWidth:true;controller:controller;message:({id:"message",sender_name:"Alex",kind:"unknown",sent:false,revision:1})}
  Text{objectName:"receivedBody";Layout.fillWidth:true;wrapMode:Text.Wrap;text:"Hi, I reviewed the draft. The revised design is approved. Can we meet on Friday at 10 to discuss implementation?";color:Theme.text;font.family:Theme.fontFamily;font.pixelSize:Theme.fontSize(15)}
  MessageAssistance{objectName:"sentActions";Layout.fillWidth:true;controller:controller;message:({id:"sent-message",sender_name:"Me",kind:"unknown",sent:true,revision:1})}
 }
}}}
})",QUrl::fromLocalFile(temp.filePath("Harness.qml")));
 require(!qml.rootObjects().isEmpty(),"Local model QML loads");auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());auto panel=find(window->contentItem(),"panel");require(panel,"panel exists");
 await([&]{return panel->property("revision").toInt()==0 && panel->property("status").toString().contains("fixture");});
 require(backend.inferences==0,"Opening settings must not infer");
 const auto settingsCapture=qEnvironmentVariable("FM_SETTINGS_CAPTURE");if(!settingsCapture.isEmpty()){QCoreApplication::processEvents();require(window->grabWindow().save(settingsCapture),"capture native model settings");}
 auto combo=find(window->contentItem(),"mailModel");require(combo,"summary selector");require(combo->property("count").toInt()==2,"cloud model excluded from selectable choices");
 combo->setProperty("currentIndex",1);
 require(QMetaObject::invokeMethod(combo,"activated",Q_ARG(int,1)),"native selector signal");
 const auto originalRequest=QJsonDocument::fromJson(backend.settings["prompt"].toString().toUtf8()).toVariant().toMap();
 const auto originalMessages=originalRequest["messages"].toList();
 auto *modelChoice=find(window->contentItem(),"mailModel");require(modelChoice,"native model choice");
 await([&]{return panel->property("discovered").toBool();});
 modelChoice->forceActiveFocus();
 QKeyEvent modelDown(QEvent::KeyPress,Qt::Key_Down,Qt::NoModifier),modelUp(QEvent::KeyRelease,Qt::Key_Down,Qt::NoModifier);
 QCoreApplication::sendEvent(modelChoice,&modelDown);QCoreApplication::sendEvent(modelChoice,&modelUp);
 await([&]{return panel->property("selectedAvailable").toBool();});
 backend.delayDiscovery=true;QMetaObject::invokeMethod(panel,"refresh");
 require(modelChoice->property("currentIndex").toInt()==1 && !find(window->contentItem(),"modelUnavailable")->isVisible(),"refresh retains installed selection without a false missing warning");
 await([&]{return !backend.discoveries.empty();});backend.flush(backend.discoveries);backend.delayDiscovery=false;
 require(panel->property("selectedAvailable").toBool(),"installed model identity survives refreshed discovery");
 auto systemField=find(window->contentItem(),"mailPrompt0");auto field=find(window->contentItem(),"mailPrompt1");require(systemField && field,"system and user content editors");
 require(systemField->property("text").toString()==originalMessages[0].toMap()["content"].toString(),"system content only");
 require(field->property("text").toString()==originalMessages[1].toMap()["content"].toString(),"user content only with decoded newlines");
 require(field->property("text").toString().contains("\n\n") && !field->property("text").toString().contains("\\n"),"normal paragraph breaks, no JSON escapes");
 require(!find(window->contentItem(),"mailPrompt") && !find(window->contentItem(),"modelEnvelope"),"no request JSON or separate rules editor");
 const QString editedContent=field->property("text").toString()+"\n\nUse French. Preserve \"quotes\" and literal \\ paths.";
 field->forceActiveFocus();QMetaObject::invokeMethod(field,"selectAll");QInputMethodEvent input;input.setCommitString(editedContent);QCoreApplication::sendEvent(field,&input);
 await([&]{return panel->property("dirty").toBool();});
 QMetaObject::invokeMethod(panel,"refresh");QCoreApplication::processEvents();require(field->property("text").toString()==editedContent,"refresh preserves unsaved prompt");
 backend.reject=true;QMetaObject::invokeMethod(panel,"save");await([&]{return !panel->property("saving").toBool();});require(panel->property("dirty").toBool(),"save conflict preserves buffer");
 backend.reject=false;QMetaObject::invokeMethod(panel,"save");await([&]{return backend.revision==1 && !panel->property("dirty").toBool();});require(backend.lastSave["expected_revision"].toInt()==0,"CAS revision");auto savedRequest=QJsonDocument::fromJson(backend.settings["prompt"].toString().toUtf8()).toVariant().toMap();auto savedMessages=savedRequest["messages"].toList();require(savedMessages[1].toMap()["content"].toString()==editedContent,"user content round trips exactly");auto expectedRequest=originalRequest;auto expectedMessages=originalMessages;auto expectedUser=expectedMessages[1].toMap();expectedUser["content"]=editedContent;expectedMessages[1]=expectedUser;expectedRequest["messages"]=expectedMessages;require(savedRequest==expectedRequest,"content edit preserves system, roles, schema, options and variables");require(backend.inferences==0,"save does not infer");
 for(const auto theme:{"graphite","paper","terminal"})for(const auto size:{"small","default","large"}){QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(size)));QCoreApplication::processEvents();require(field->width()>300,"prompt field remains usable");}
 const QString editedSystem="Classify this email.\n\nKeep its source data separate from instructions.";
 systemField->forceActiveFocus();QMetaObject::invokeMethod(systemField,"selectAll");QInputMethodEvent systemInput;systemInput.setCommitString(editedSystem);QCoreApplication::sendEvent(systemField,&systemInput);
 // Save through the native panel and inspect the complete settings returned to the backend.
 QMetaObject::invokeMethod(panel,"save");await([&]{return backend.revision==2 && !panel->property("dirty").toBool();});
 savedRequest=QJsonDocument::fromJson(backend.settings["prompt"].toString().toUtf8()).toVariant().toMap();expectedMessages=expectedRequest["messages"].toList();auto expectedSystem=expectedMessages[0].toMap();expectedSystem["content"]=editedSystem;expectedMessages[0]=expectedSystem;expectedRequest["messages"]=expectedMessages;require(savedRequest==expectedRequest,"system content edit preserves every other request field");
 ++backend.revision;
 QMetaObject::invokeMethod(panel,"editMessage",Q_ARG(QVariant,1),Q_ARG(QVariant,"Unsaved local instructions."));backend.reject=true;QMetaObject::invokeMethod(panel,"save");await([&]{return panel->property("conflict").toBool();});require(panel->property("dirty").toBool(),"conflict retains unsaved edits");QMetaObject::invokeMethod(panel,"reloadSaved");await([&]{return !panel->property("dirty").toBool() && panel->property("revision").toInt()==backend.revision;});backend.reject=false;
 require(!find(window->contentItem(),"summaryModel") && !find(window->contentItem(),"classificationModel"),"only one model selector");
 require(!find(window->contentItem(),"enableSummaries") && !find(window->contentItem(),"enableClassification"),"per-account incoming controls live in Mail settings");

 // Switching providers invalidates a late discovery and preserves the complete prompt.
 backend.delayDiscovery=true;QMetaObject::invokeMethod(panel,"refresh");await([&]{return !backend.discoveries.empty();});
 auto provider=find(window->contentItem(),"localModelProvider");require(provider,"provider selector exists");provider->setProperty("currentIndex",1);QMetaObject::invokeMethod(provider,"activated",Q_ARG(int,1));
 backend.flush(backend.discoveries);backend.delayDiscovery=false;
 require(combo->property("count").toInt()==1,"old provider discovery cannot populate new provider choices");
 require(panel->property("draft").value<QJSValue>().toVariant().toMap()["model"].isNull(),"provider change clears incompatible selection");
 QMetaObject::invokeMethod(panel,"refresh");await([&]{return panel->property("status").toString().contains("Local server") && combo->property("count").toInt()==2;});
 require(backend.lastDiscovery["provider"]=="openai_compatible" && backend.lastDiscovery["endpoint"]=="http://127.0.0.1:1234","discovery uses chosen provider and endpoint");
 combo->setProperty("currentIndex",1);QMetaObject::invokeMethod(combo,"activated",Q_ARG(int,1));
 const auto providerRevision=backend.revision;QMetaObject::invokeMethod(panel,"save");await([&]{return backend.revision==providerRevision+1 && !panel->property("dirty").toBool();});
 require(backend.settings["provider"]=="openai_compatible" && backend.settings["model"].toMap()["digest"].toString().isEmpty(),"server identity saved without a fabricated weight digest");
 require(QJsonDocument::fromJson(backend.settings["prompt"].toString().toUtf8()).toVariant().toMap()==expectedRequest,"provider switch preserves system/user content, schema, and variables");
 require(backend.inferences==0,"provider setup never starts historical inference");
 const auto compatibleCapture=qEnvironmentVariable("FM_COMPATIBLE_SETTINGS_CAPTURE");
 if(!compatibleCapture.isEmpty()){QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString("graphite")),Q_ARG(QVariant,QString("default")));window->resize(680,900);QElapsedTimer paint;paint.start();while(paint.elapsed()<100)QCoreApplication::processEvents();require(window->grabWindow().save(compatibleCapture),"capture compatible provider settings");}

 backend.settings["accounts"]=QVariantMap{{"personal",QVariantMap{{"summaries",true},{"classification",false},{"descriptions",QVariantMap{}}}}};
 QMetaObject::invokeMethod(window,"showReader");await([&]{return find(window->contentItem(),"controller")!=nullptr;});auto controller=find(window->contentItem(),"controller");
 auto invokeStart=[&](QString task,QString target){require(QMetaObject::invokeMethod(controller,"start",Q_ARG(QVariant,task),Q_ARG(QVariant,target)),"start method");};
 await([&]{return find(window->contentItem(),"summarizeMessage_message")->isVisible();});
 require(!find(window->contentItem(),"correctSender_message")->isVisible(),"summary-only mode has no classification controls");
 require(!find(window->contentItem(),"sentActions")->isVisible(),"sent mail has no assistance");
 require(backend.inferences==0,"opening reader must not start historical inference");
 await([&]{return backend.watches>0;});const auto watchesBefore=backend.watches;++backend.revision;emit backend.conversationChanged();await([&]{return backend.watches==watchesBefore+1 && controller->property("settingsRevision").toInt()==backend.revision;});const auto watchesAfter=backend.watches;emit backend.conversationChanged();QCoreApplication::processEvents();require(backend.watches==watchesAfter,"unchanged Settings refresh does not reset new-mail cursor");
 auto card=find(window->contentItem(),"summaryFixture");require(card,"thread card");
 await([&]{return !card->property("summaryText").toString().isEmpty();});
 auto messageCard=find(window->contentItem(),"messageSummary_message");auto action=find(window->contentItem(),"summarizeMessage_message");auto receivedBody=find(window->contentItem(),"receivedBody");
 auto settle=[] {QElapsedTimer timer;timer.start();while(timer.elapsed()<300){QCoreApplication::processEvents();QThread::msleep(5);}};
 settle();const auto idleHeight=messageCard->height(),idleBodyY=receivedBody->y(),idleWidth=action->width();
 const auto motionCapture=qEnvironmentVariable("FM_SUMMARY_MOTION_CAPTURE");
 auto capture=[&](const QString &state){if(!motionCapture.isEmpty())require(window->grabWindow().save(motionCapture+"-"+state+".png"),"summary transition capture");};
 capture("idle");backend.delayStart=true;backend.delayPoll=true;
 QMetaObject::invokeMethod(action,"clicked");await([&]{return backend.starts.size()==1;});settle();
 require(!action->isVisible() && find(messageCard,"summaryProgress")->isVisible(),"busy summary replaces compact action with the dashed progress box");
 require(messageCard->height()>idleHeight && !messageCard->property("heightAnimating").toBool(),"busy box has immediate stable geometry");
 const auto busyHeight=messageCard->height(),busyBodyY=receivedBody->y();
 require(!find(window->contentItem(),"classificationError_message")->isVisible(),"no external combined-job feedback");
 capture("queued");backend.flush(backend.starts);backend.delayStart=false;await([&]{return !backend.polls.empty();});settle();
 require(qAbs(receivedBody->y()-busyBodyY)<0.5,"running job retains busy-box geometry");capture("running");
 backend.delayResults=true;backend.flush(backend.polls);backend.delayPoll=false;await([&]{return !backend.results.empty();});settle();
 require(controller->property("busy").toBool() && !action->isVisible(),"completion waits for committed result without flashing idle control");
 require(qAbs(messageCard->height()-busyHeight)<0.5,"result readback preserves processing height");
 backend.delayResults=false;backend.flush(backend.results);settle();
 require(!messageCard->property("heightAnimating").toBool() && !action->isVisible(),"summary text appears without moving the header through animation");
 require(messageCard->property("summaryText").toString()==backend.summaryText,"committed message summary appears in same surface");capture("completed");
 await([&]{return backend.inferences==1 && !controller->property("busy").toBool();});
 require(backend.targets["job-1"].toMap()["message_id"]=="message","message button preserves exact target");
 require(controller->property("status").toString()=="completed","reader completion");
 // Source disclosure addresses the selected retained message, without a generic duplicate button.
 card->setProperty("details",true);await([&]{auto item=find(window->contentItem(),"summarySource_message");return item && item->isVisible();});auto source=find(window->contentItem(),"summarySource_message");require(source && source->isVisible(),"source disclosure visible");QMetaObject::invokeMethod(source,"clicked");require(window->property("openedSource")=="message","exact source identity");card->setProperty("details",false);
 // Rapid activation admits one job; navigating/canceling before its reply cancels the late job.
 backend.delayStart=true;invokeStart("summary","");invokeStart("summary","");await([&]{return backend.starts.size()==1;});require(backend.inferences==2,"duplicate activation suppressed while starting");QMetaObject::invokeMethod(controller,"cancel");backend.selected="other";emit backend.selectionChanged();backend.flush(backend.starts);await([&]{return backend.cancellations>0;});require(!controller->property("busy").toBool(),"late start does not attach to another conversation");
 backend.delayStart=false;backend.selected="conversation";emit backend.selectionChanged();
 // An old poll cannot complete a replacement job.
 backend.delayPoll=true;invokeStart("summary","");await([&]{return !backend.polls.empty();});QMetaObject::invokeMethod(controller,"cancel");invokeStart("summary","");await([&]{return controller->property("job").toString()=="job-4";});backend.flush(backend.polls);require(controller->property("job").toString()=="job-4","old poll cannot clear new job");backend.delayPoll=false;await([&]{return !controller->property("busy").toBool();});
 backend.failPoll=true;invokeStart("summary","message");await([&]{return !controller->property("busy").toBool();});require(!controller->property("error").toString().isEmpty(),"poll failure releases busy state with visible error");require(find(window->contentItem(),"messageSummary_message")->property("error").toString()==controller->property("error").toString(),"error shown in this message summary");require(!find(window->contentItem(),"classificationError_message")->isVisible(),"combined error not repeated beside summary card");backend.failPoll=false;
 backend.delayPoll=true;invokeStart("summary","");await([&]{return !backend.polls.empty();});backend.online=false;emit backend.connectedChanged();require(!controller->property("busy").toBool(),"disconnect releases busy state");backend.flush(backend.polls);backend.delayPoll=false;backend.online=true;emit backend.connectedChanged();invokeStart("summary","");await([&]{return !controller->property("busy").toBool();});
 // Regeneration keeps the existing text and height; cancellation lives only in its menu.
 settle();backend.delayPoll=true;const auto readyHeight=messageCard->height(),readyBodyY=receivedBody->y();
 invokeStart("summary","message");await([&]{return !backend.polls.empty();});settle();
 qInfo()<<"Regeneration geometry"<<readyHeight<<messageCard->height()<<readyBodyY<<receivedBody->y()<<messageCard->property("displayedError");
 require(messageCard->property("summaryText").toString()==backend.summaryText && find(messageCard,"summaryProgress")->isVisible(),"regeneration retains saved result and shows the progress box");
 require(!find(messageCard,"summaryCancel"),"cancel is absent from the inline summary surface");
 QMetaObject::invokeMethod(find(messageCard,"summaryActions"),"clicked");
 await([&]{auto item=find(window->contentItem(),"summaryCancel");return item && item->isVisible();});
 QMetaObject::invokeMethod(find(window->contentItem(),"summaryCancel"),"clicked");await([&]{return !controller->property("busy").toBool();});
 backend.flush(backend.polls);backend.delayPoll=false;settle();
 require(qAbs(messageCard->height()-readyHeight)<0.5,"cancel leaves existing summary geometry intact");
 // A first-attempt error stays inside the same surface and retry does not collapse it.
 auto receivedActions=find(window->contentItem(),"receivedActions");
 receivedActions->setProperty("message",QVariantMap{{"id","fresh-message"},{"sender_name","Alex"},{"kind","unknown"},{"sent",false},{"revision",1}});
 await([&]{return find(window->contentItem(),"messageSummary_fresh-message")!=nullptr;});
 auto fresh=find(window->contentItem(),"messageSummary_fresh-message");backend.failPoll=true;invokeStart("summary","fresh-message");await([&]{return !controller->property("busy").toBool();});settle();
 require(find(fresh,"summaryError")->isVisible() && !find(window->contentItem(),"classificationError_fresh-message")->isVisible(),"first failure is contained without duplicate external feedback");if(!motionCapture.isEmpty())require(window->grabWindow().save(motionCapture+"-failed.png"),"summary failure capture");
 const auto errorHeight=fresh->height(),errorBodyY=receivedBody->y();backend.failPoll=false;backend.delayPoll=true;
 invokeStart("summary","fresh-message");await([&]{return !backend.polls.empty();});settle();
 qInfo()<<"Retry geometry"<<errorHeight<<fresh->height()<<errorBodyY<<receivedBody->y()<<fresh->property("displayedError");
 require(fresh->height()>26 && find(fresh,"summaryProgress")->isVisible(),"retry shows the full progress surface");
 backend.flush(backend.polls);backend.delayPoll=false;await([&]{return !controller->property("busy").toBool();});settle();
 require(!find(fresh,"summaryError")->isVisible() && fresh->property("summaryText").toString()==backend.summaryText,"successful retry replaces error with summary");
 // Navigating to a cached summary must never arm a reveal animation, even after delayed readback.
 settle();auto snapshot=backend.summary("cached-message");
 receivedActions->setProperty("message",QVariantMap{{"id","cached-message"},{"sender_name","Alex"},{"kind","unknown"},{"sent",false},{"revision",1}});
 backend.completedItems["cached-message"]=snapshot;backend.delayResults=true;emit backend.conversationChanged();await([&]{return !backend.results.empty();});settle();
 auto cached=find(window->contentItem(),"messageSummary_cached-message");require(cached && !cached->property("motionReady").toBool(),"navigation does not arm summary animation");
 backend.delayResults=false;backend.flush(backend.results);QCoreApplication::processEvents();
 bool cachedPaint=false;const auto paintConnection=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{cachedPaint=true;},Qt::QueuedConnection);window->requestUpdate();await([&]{return cachedPaint;});QObject::disconnect(paintConnection);
 const auto cachedHeight=cached->height();require(!cached->property("heightAnimating").toBool(),"cached summary does not start a height animation");require(cached->property("summaryText").toString()==backend.summaryText && !cached->property("motionReady").toBool(),"cached result is immediately visible without animation");
 settle();require(qAbs(cachedHeight-cached->height())<0.5,"saved summary has no delayed height growth");
 // Geometry at narrow reader widths and every supported appearance, using production QML.
 const auto screenshots=qEnvironmentVariable("FM_ASSIST_SCREENSHOTS");if(!screenshots.isEmpty())QDir().mkpath(screenshots);
 for(const auto theme:{"graphite","paper","terminal"})for(const auto size:{"small","default","large"}){
  window->setWidth(480);QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(size)));QCoreApplication::processEvents();
  auto body=find(card,"summaryBody");require(body && body->width()<=card->width()-28 && body->width()>300,"summary fits narrow card");
  require(card->height()>70 && card->height()<300,"compact summary card");
  if(!screenshots.isEmpty() && QString(size)=="default"){window->setWidth(720);QCoreApplication::processEvents();QThread::msleep(30);QCoreApplication::processEvents();require(window->grabWindow().save(screenshots+"/summary-"+theme+".png"),"capture actual native frame");}
 }
 backend.summaryText=QString(600,'W');emit backend.conversationChanged();await([&]{return card->property("summaryText").toString().size()==600;});QCoreApplication::processEvents();require(find(card,"summaryBody")->width()<=card->width()-28 && card->height()>70,"long summary wraps within the padded full-width column");
 require(!warnings,"production QML has no runtime warnings");qInfo()<<"Local assistance passed: Settings edits/CAS, incoming-only controls, exact targets/sources, start/poll/cancel/switch/disconnect races, and all theme/text-size geometry";return 0;
}
#include "LocalModelTest.moc"
