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
#include <QFontDatabase>
#include <QTimer>
#include <QElapsedTimer>
#include <QThread>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QDebug>
#include <QMap>
static void require(bool ok,const char* message){if(!ok)qFatal("%s",message);}
template<typename F> static void waitFor(F predicate){QElapsedTimer t;t.start();while(!predicate()&&t.elapsed()<5000){QCoreApplication::processEvents();QThread::msleep(5);}require(predicate(),"label control wait timed out");}
static QQuickItem* find(QQuickItem* root,const QString& name){if(root->objectName()==name)return root;for(auto child:root->childItems())if(auto match=find(child,name))return match;return nullptr;}
class LabelBackend:public QObject {
    Q_OBJECT
    Q_PROPERTY(bool connected MEMBER connected NOTIFY connectedChanged)
    Q_PROPERTY(QString account MEMBER account)
    Q_PROPERTY(QVariantList accounts MEMBER accounts)
public:
    bool connected=true,reject=false;QString account="work",lastAccount,lastRemovalAccount;int sequence=0,created=0,described=0,autoReads=0,removed=0,synced=0;
    QVariantList accounts{QVariantMap{{"id","work"},{"name","Work"}},QVariantMap{{"id","personal"},{"name","Personal"}}};
    QMap<QString,QVariantList> labels;
    Q_INVOKABLE bool accountCapability(const QString&,const QString&){return true;}
    Q_INVOKABLE QString labelRequest(QString method,QVariantMap params,QString scope){
        const auto ticket="request-"+QString::number(++sequence);
        QTimer::singleShot(0,this,[this,ticket,method,params,scope]{
            QVariantMap result;bool ok=true;lastAccount=scope;
            if(method=="label_catalog")result={{"categories",labels[scope]}};
            else if(method=="label_create"){
                ++created;const auto id=params.value("id","label-"+ticket).toString();labels[scope].append(QVariantMap{{"id",id},{"name",params["name"]},{"description",""},{"auto_read",false},{"conversation_count",0}});result={{"id",id}};
            }else if(method=="label_describe"){
                if(reject){ok=false;result={{"message","Not saved — fixture conflict"}};}
                else{++described;for(auto &row:labels[scope]){auto label=row.toMap();if(label["id"]==params["id"]){label["description"]=params["description"];row=label;}}}
            }else if(method=="label_auto_read"){
                ++autoReads;for(auto &row:labels[scope]){auto label=row.toMap();if(label["id"]==params["id"]){require(label["auto_read"]==params["expected_auto_read"],"Auto-read uses captured previous state");label["auto_read"]=params["auto_read"];row=label;}}
            }else if(method=="label_remove"){
                ++removed;lastRemovalAccount=scope;auto &rows=labels[scope];for(int i=0;i<rows.size();++i)if(rows[i].toMap()["id"]==params["id"]){require(rows[i].toMap()["name"]==params["expected_name"],"captured label name");rows.removeAt(i);break;}
            }else if(method=="google_labels_sync"){++synced;result={{"state","running"},{"job_id","sync-job"}};}
            else if(method=="google_labels_sync_status")result={{"state","complete"},{"job_id","sync-job"},{"result",QVariantMap{{"message","Already in sync · just now"}}}};
            emit labelReply(ticket,method,result,ok);
        });return ticket;
    }
signals:
    void connectedChanged();void labelReply(QString ticket,QString method,QVariantMap result,bool ok);
};
int main(int argc,char**argv){
    QGuiApplication app(argc,argv);QQuickStyle::setStyle("Basic");
    for(const auto font:{"Geist-Regular.ttf","Geist-Medium.ttf","Geist-SemiBold.ttf"})require(QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR)+"/fonts/"+font)>=0,"bundled fonts");QGuiApplication::setFont(QFont("Geist",10));
    QTemporaryDir temp;auto path=temp.filePath("LabelTest");require(QDir().mkpath(path),"module directory");
    QFile module(path+"/qmldir");require(module.open(QIODevice::WriteOnly),"module metadata");module.write("module LabelTest\n");
    for(const auto& file:QDir(QString(FM_GUI_SOURCE_DIR)).entryList({"*.qml"},QDir::Files)){require(QFile::copy(QString(FM_GUI_SOURCE_DIR)+"/"+file,path+"/"+file),"copy production QML");auto name=QFileInfo(file).completeBaseName();module.write(((name=="Theme"?"singleton ":"")+name+" 1.0 "+file+"\n").toUtf8());}module.close();
    LabelBackend backend;QQmlApplicationEngine qml; FmIconItem::registerType();bool warnings=false;QObject::connect(&qml,&QQmlEngine::warnings,[&](auto errors){warnings=true;for(const auto&e:errors)qWarning()<<e;});qml.addImportPath(temp.path());qml.rootContext()->setContextProperty("backend",&backend);
    qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport LabelTest\nApplicationWindow{width:760;height:640;visible:true;color:Theme.canvas;LabelSettings{objectName:'panel';x:40;y:30;width:680}}",QUrl::fromLocalFile(temp.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(),"label QML loads");auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());
    auto control=[&](QString name){auto item=find(window->contentItem(),name);require(item,"label control exists");return item;};
    auto type=[&](QQuickItem*field,QString value){field->forceActiveFocus();QMetaObject::invokeMethod(field,"selectAll");QInputMethodEvent input;input.setCommitString(value);QCoreApplication::sendEvent(field,&input);};
    auto enter=[&](QQuickItem*field){QKeyEvent key(QEvent::KeyPress,Qt::Key_Return,Qt::NoModifier);QCoreApplication::sendEvent(field,&key);};
    auto panel=control("panel");waitFor([&]{return backend.sequence>0;});QCoreApplication::processEvents();
    auto name=control("newLabelName");type(name,"Research");enter(name);waitFor([&]{return backend.created==1&&!panel->property("busy").toBool();});
    require(backend.labels["work"].size()==1&&backend.labels["personal"].isEmpty(),"Enter adds only to selected account");
    auto id=backend.labels["work"].first().toMap()["id"].toString();waitFor([&]{return find(window->contentItem(),"labelDescription_"+id)!=nullptr;});
    auto description=control("labelDescription_"+id);backend.reject=true;type(description,"Papers and findings");enter(description);waitFor([&]{return panel->property("status").toString().startsWith("Not saved");});require(description->property("text")=="Papers and findings","conflict preserves instructions");
    backend.reject=false;QMetaObject::invokeMethod(control("retryLabelChange"),"clicked");waitFor([&]{return backend.described==1&&!panel->property("busy").toBool();});require(backend.labels["work"].first().toMap()["description"]=="Papers and findings","retry saves captured text");
    auto autoRead=control("labelAutoRead_"+id);autoRead->setProperty("checked",true);QMetaObject::invokeMethod(autoRead,"clicked");waitFor([&]{return backend.autoReads==1&&!panel->property("busy").toBool();});
    require(backend.labels["work"].first().toMap()["auto_read"]==true&&backend.labels["personal"].isEmpty(),"Auto-read is saved only for selected account and label");
    const auto capture=qEnvironmentVariable("FM_LABEL_SETTINGS_CAPTURE");if(!capture.isEmpty()){QElapsedTimer paint;paint.start();while(paint.elapsed()<100)QCoreApplication::processEvents();require(window->grabWindow().save(capture),"label settings frame");}
    QMetaObject::invokeMethod(control("removeLabel_"+id),"clicked");waitFor([&]{return find(window->contentItem(),"confirmRemoveLabel")!=nullptr;});
    panel->setProperty("account","personal");QMetaObject::invokeMethod(control("confirmRemoveLabel"),"clicked");waitFor([&]{return backend.removed==1;});require(backend.lastRemovalAccount=="work"&&backend.labels["work"].isEmpty(),"confirmation retains original account through selection change");
    waitFor([&]{return !panel->property("busy").toBool();});QMetaObject::invokeMethod(control("syncAccountLabels"),"clicked");waitFor([&]{return backend.synced==1;});require(backend.lastAccount=="personal","sync uses selected account");
    waitFor([&]{return !panel->property("busy").toBool()&&panel->property("status").toString().startsWith("Already in sync");});
    require(!warnings,"label settings have no QML warnings");return 0;
}
#include "LabelControlsTest.moc"
