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
#include <QJsonDocument>
#include <QJSValue>
#include <QDebug>
#include <QFontDatabase>
#include <functional>
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
 QVariantMap settings{{"endpoint","http://127.0.0.1:11434"},{"summary_model",QVariant()},{"classification_model",QVariant()},{"summary_prompt","Summarize faithfully."},{"classification_prompt","Classify configured labels."},{"accounts",QVariantMap{}}};
 int sequence=0,revision=0,inferences=0,cancellations=0,watches=0;bool reject=false,delayStart=false,delayPoll=false,failPoll=false;QVariantMap lastSave;
 std::vector<std::function<void()>> starts,polls;
 QVariantMap targets;
 QString summaryText="Alex approved the revised design. Meet on Friday at 10 to discuss implementation; the time is still a proposal.";
 void flush(std::vector<std::function<void()>> &queue){auto pending=std::move(queue);queue.clear();for(auto &reply:pending)reply();}
 QVariantMap summary(QString target={})const{return {{"task","summary"},{"target",target},{"output",QVariantMap{{"summary",summaryText},{"source_ids",QVariantList{"message"}}}},{"source_ids",QVariantList{"message"}},{"stale",false},{"partial",false},{"model",QVariantMap{{"name","gemma3n:e4b"}}},{"created_at","2026-09-14T22:00:00Z"}};}

 Q_INVOKABLE QString assistanceRequest(QString method,QVariantMap params,QString = {}){
  const auto ticket=QString::number(++sequence);
  QTimer::singleShot(0,this,[this,ticket,method,params]{
   QVariantMap result;bool ok=true;
   if(method=="assist_settings_get")result={{"revision",revision},{"settings",settings},{"defaults",settings}};
   else if(method=="assist_discover")result={{"version","fixture"},{"models",QVariantList{QVariantMap{{"name","local:latest"},{"digest",QString(64,'a')},{"eligible",true}},QVariantMap{{"name","remote:cloud"},{"digest",QString(64,'b')},{"eligible",false},{"reason","Cloud model"}}}}};
   else if(method=="assist_watch"){++watches;result={{"watching",false}};}
   else if(method=="assist_catalog")result={{"categories",categories()}};
   else if(method=="assist_results")result={{"items",selected=="conversation" ? QVariantList{summary()} : QVariantList{}}};
   else if(method=="assist_settings_save"){
    if(reject){ok=false;result={{"message","Settings changed; reload"}};}
    else{lastSave=params;settings=params["settings"].toMap();++revision;result={{"revision",revision},{"settings",settings},{"defaults",settings}};}
   }else if(method=="assist_start"){
    ++inferences;auto id="job-"+QString::number(inferences);targets[id]=params;result={{"job_id",id},{"status","queued"}};
    if(delayStart){starts.push_back([this,ticket,method,result]{emit assistanceReply(ticket,method,result,true);});return;}
   }else if(method=="assist_cancel"){++cancellations;result={{"status","canceled"}};}
   else if(method=="assist_job"){
    if(failPoll){ok=false;result={{"message","The local model disconnected. Try again."}};}
    else result={{"status","completed"},{"items",QVariantList{summary(targets[params["job_id"].toString()].toMap()["message_id"].toString())}}};
    if(delayPoll){polls.push_back([this,ticket,method,result,ok]{emit assistanceReply(ticket,method,result,ok);});return;}
   }
   emit assistanceReply(ticket,method,result,ok);
  });return ticket;
 }
 Q_INVOKABLE void refresh(){}
signals:
 void assistanceReply(QString ticket,QString method,QVariantMap result,bool ok);
 void connectedChanged();void conversationChanged();void selectionChanged();
};
int main(int argc,char **argv){
 QGuiApplication app(argc,argv);QQuickStyle::setStyle("Basic");
 for(const auto file:{"Geist-Regular.ttf","Geist-Medium.ttf","Geist-SemiBold.ttf"})require(QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR)+"/fonts/"+file)>=0,"load shipped font");
 Backend backend;QTemporaryDir temp;auto dir=temp.filePath("ModelTest");require(QDir().mkpath(dir),"module directory");
 for(const auto name:{"Theme.qml","FmButton.qml","FmToolTip.qml","FmIcon.qml","FmComboBox.qml","FmTextField.qml","LocalModelSettings.qml","MailAssistance.qml","SummaryCard.qml","MessageAssistance.qml"})require(QFile::copy(QString(FM_GUI_SOURCE_DIR)+"/"+name,dir+"/"+name),"copy production QML");
 QFile module(dir+"/qmldir");require(module.open(QIODevice::WriteOnly),"module metadata");module.write("module ModelTest\nsingleton Theme 1.0 Theme.qml\nLocalModelSettings 1.0 LocalModelSettings.qml\nMailAssistance 1.0 MailAssistance.qml\nSummaryCard 1.0 SummaryCard.qml\nMessageAssistance 1.0 MessageAssistance.qml\n");module.close();
 QQmlApplicationEngine qml;bool warnings=false;QObject::connect(&qml,&QQmlEngine::warnings,[&](auto errors){warnings=true;for(const auto &e:errors)qWarning()<<e;});qml.addImportPath(temp.path());qml.rootContext()->setContextProperty("backend",&backend);
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
  SummaryCard{objectName:"threadSummary";Layout.fillWidth:true;result:controller.resultFor("summary","");busy:controller.workingOn("summary","");error:controller.errorFor("summary","");canSummarize:controller.summariesEnabled && !controller.busy;members:[{id:"message",sender_name:"Alex",sent_at:"2026-09-14T22:00:00Z"}];onRegenerate:controller.start("summary","");onCancelRequested:controller.cancel();onOpenSource:id=>openedSource=id}
  Text{text:"Alex";color:Theme.text;font.family:Theme.fontFamily;font.pixelSize:Theme.fontSize(13);font.weight:Font.Medium}
  MessageAssistance{objectName:"receivedActions";Layout.fillWidth:true;controller:controller;message:({id:"message",sender_name:"Alex",kind:"unknown",sent:false,revision:1})}
  Text{Layout.fillWidth:true;wrapMode:Text.Wrap;text:"Hi, I reviewed the draft. The revised design is approved. Can we meet on Friday at 10 to discuss implementation?";color:Theme.text;font.family:Theme.fontFamily;font.pixelSize:Theme.fontSize(15)}
  MessageAssistance{objectName:"sentActions";Layout.fillWidth:true;controller:controller;message:({id:"sent-message",sender_name:"Me",kind:"unknown",sent:true,revision:1})}
 }
}}}
})",QUrl::fromLocalFile(temp.filePath("Harness.qml")));
 require(!qml.rootObjects().isEmpty(),"Local model QML loads");auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());auto panel=find(window->contentItem(),"panel");require(panel,"panel exists");
 await([&]{return panel->property("revision").toInt()==0 && panel->property("status").toString().contains("fixture");});
 require(backend.inferences==0,"Opening settings must not infer");
 auto combo=find(window->contentItem(),"summaryModel");require(combo,"summary selector");require(combo->property("count").toInt()==2,"cloud model excluded from selectable choices");
 combo->setProperty("currentIndex",1);
 require(QMetaObject::invokeMethod(combo,"activated",Q_ARG(int,1)),"native selector signal");
 auto field=find(window->contentItem(),"summaryPrompt");require(field,"editable summary instructions");field->forceActiveFocus();QMetaObject::invokeMethod(field,"selectAll");QInputMethodEvent input;input.setCommitString("Use two concise French sentences.");QCoreApplication::sendEvent(field,&input);
 await([&]{return panel->property("dirty").toBool();});
 QMetaObject::invokeMethod(panel,"refresh");QCoreApplication::processEvents();require(field->property("text").toString()=="Use two concise French sentences.","refresh preserves unsaved prompt");
 backend.reject=true;QMetaObject::invokeMethod(panel,"save");await([&]{return !panel->property("saving").toBool();});require(panel->property("dirty").toBool(),"save conflict preserves buffer");
 backend.reject=false;QMetaObject::invokeMethod(panel,"save");await([&]{return backend.revision==1 && !panel->property("dirty").toBool();});require(backend.lastSave["expected_revision"].toInt()==0,"CAS revision");require(backend.settings["summary_prompt"].toString()=="Use two concise French sentences.","prompt saved");require(backend.inferences==0,"save does not infer");
 for(const auto theme:{"graphite","paper","harbour"})for(const auto size:{"small","default","large"}){QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(size)));QCoreApplication::processEvents();require(field->width()>300,"prompt field remains usable");}
 backend.settings["summary_prompt"]="Saved in another client.";++backend.revision;
 QMetaObject::invokeMethod(panel,"edit",Q_ARG(QVariant,"summary_prompt"),Q_ARG(QVariant,"Unsaved local instructions."));backend.reject=true;QMetaObject::invokeMethod(panel,"save");await([&]{return panel->property("conflict").toBool();});require(panel->property("dirty").toBool(),"conflict retains unsaved edits");QMetaObject::invokeMethod(panel,"reloadSaved");await([&]{return !panel->property("dirty").toBool() && panel->property("revision").toInt()==backend.revision;});backend.reject=false;
 backend.delayStart=true;QMetaObject::invokeMethod(panel,"sample",Q_ARG(QVariant,"summary"));QMetaObject::invokeMethod(panel,"sample",Q_ARG(QVariant,"summary"));await([&]{return backend.starts.size()==1;});require(backend.inferences==1,"sample double activation admits one job");QMetaObject::invokeMethod(panel,"cancelSample");backend.flush(backend.starts);await([&]{return backend.cancellations==1;});require(!panel->property("sampleBusy").toBool(),"late sample start is canceled");backend.delayStart=false;backend.inferences=0;backend.cancellations=0;backend.targets.clear();
 backend.settings["accounts"]=QVariantMap{{"personal",QVariantMap{{"summaries",true},{"classification",false},{"descriptions",QVariantMap{}}}}};
 QMetaObject::invokeMethod(window,"showReader");await([&]{return find(window->contentItem(),"controller")!=nullptr;});auto controller=find(window->contentItem(),"controller");
 auto invokeStart=[&](QString task,QString target){require(QMetaObject::invokeMethod(controller,"start",Q_ARG(QVariant,task),Q_ARG(QVariant,target)),"start method");};
 await([&]{return find(window->contentItem(),"summarizeMessage_message")->isVisible();});
 require(!find(window->contentItem(),"correctSender_message")->isVisible(),"summary-only mode has no classification controls");
 require(find(window->contentItem(),"summarizeMessage_sent-message")->isVisible(),"sent mail can be summarized independently");
 require(backend.inferences==0,"opening reader must not start historical inference");
 await([&]{return backend.watches>0;});const auto watchesBefore=backend.watches;++backend.revision;emit backend.conversationChanged();await([&]{return backend.watches==watchesBefore+1 && controller->property("settingsRevision").toInt()==backend.revision;});const auto watchesAfter=backend.watches;emit backend.conversationChanged();QCoreApplication::processEvents();require(backend.watches==watchesAfter,"unchanged Settings refresh does not reset new-mail cursor");
 auto card=find(window->contentItem(),"threadSummary");require(card,"thread card");
 await([&]{return !card->property("summaryText").toString().isEmpty();});
 QMetaObject::invokeMethod(find(window->contentItem(),"summarizeMessage_message"),"clicked");
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
 backend.failPoll=true;invokeStart("summary","");await([&]{return !controller->property("busy").toBool();});require(!controller->property("error").toString().isEmpty(),"poll failure releases busy state with visible error");backend.failPoll=false;
 backend.delayPoll=true;invokeStart("summary","");await([&]{return !backend.polls.empty();});backend.online=false;emit backend.connectedChanged();require(!controller->property("busy").toBool(),"disconnect releases busy state");backend.flush(backend.polls);backend.delayPoll=false;backend.online=true;emit backend.connectedChanged();invokeStart("summary","");await([&]{return !controller->property("busy").toBool();});
 // Geometry at narrow reader widths and every supported appearance, using production QML.
 const auto screenshots=qEnvironmentVariable("FM_ASSIST_SCREENSHOTS");if(!screenshots.isEmpty())QDir().mkpath(screenshots);
 for(const auto theme:{"graphite","paper","harbour"})for(const auto size:{"small","default","large"}){
  window->setWidth(480);QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(size)));QCoreApplication::processEvents();
  auto body=find(card,"summaryBody");require(body && body->width()<=card->width()-35 && body->width()>300,"summary fits narrow card");
  require(card->height()>70 && card->height()<300,"compact summary card");
  if(!screenshots.isEmpty() && QString(size)=="default"){window->setWidth(720);QCoreApplication::processEvents();QThread::msleep(30);QCoreApplication::processEvents();require(window->grabWindow().save(screenshots+"/summary-"+theme+".png"),"capture actual native frame");}
 }
 backend.summaryText=QString(8000,'W');emit backend.conversationChanged();await([&]{return card->property("summaryText").toString().size()==8000;});QCoreApplication::processEvents();require(find(card,"summaryScroll")->height()<=211,"long summary has bounded scroll");
 require(!warnings,"production QML has no runtime warnings");qInfo()<<"Local assistance passed: Settings edits/CAS, summary-only received/sent controls, exact targets/sources, start/poll/cancel/switch/disconnect races, and all theme/text-size geometry";return 0;
}
#include "LocalModelTest.moc"
