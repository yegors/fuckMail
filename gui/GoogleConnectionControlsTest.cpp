#include "FmIconItem.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QQuickStyle>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QQuickItem>
#include <QTemporaryDir>
#include <QDir>
#include <QFile>
#include <QThread>
#include <QImage>
#include <QDebug>
static void require(bool ok, const char *message) { if (!ok) qFatal("%s", message); }
class ConnectionBackend : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool connected MEMBER connected NOTIFY connectedChanged)
    Q_PROPERTY(bool googleAuthBusy MEMBER busy NOTIFY googleAuthChanged)
    Q_PROPERTY(QVariantMap googleAuth MEMBER auth NOTIFY googleAuthChanged)
    Q_PROPERTY(QVariantMap googleAuthConfiguration MEMBER configuration NOTIFY googleAuthChanged)
    Q_PROPERTY(QVariantList accounts MEMBER accounts NOTIFY accountsChanged)
    // Read-only, as in MailBackend: QML selects an account through selectAccount().
    Q_PROPERTY(QString account READ currentAccount NOTIFY accountChanged)
    Q_PROPERTY(QString preferencesStatus MEMBER preferencesStatus NOTIFY preferencesChanged)
public:
    bool connected = true, busy = false, saveSucceeds = false, analyzeHistory = false;
    QVariantMap auth, configuration{{"available",true}}, requested;
    QVariantList accounts{QVariantMap{{"id","fixture-account"},{"name","Personal"},{"email","person@example.test"},{"signature_text","Original signature"}}};
    QString account, email, savedName, syncedAccount, preferencesStatus, savedRetention;
    int connections=0, syncStarts=0, saves=0, tickets=0;
    QString currentAccount() const { return account; }
    Q_INVOKABLE void selectAccount(const QString &id) { account=id; emit accountChanged(); }
    Q_INVOKABLE QString assistanceRequest(const QString &method,const QVariantMap &,const QString &) {
        require(method=="assist_settings_get","dialog only reads model settings");
        const auto ticket=QString("model-%1").arg(++tickets);
        QMetaObject::invokeMethod(this,[this,ticket,method] { emit assistanceReply(ticket,method,{{"settings",QVariantMap{{"model",QVariantMap{{"name","fixture"},{"digest",QString(64,'a')}}}}}},true); },Qt::QueuedConnection);
        return ticket;
    }
    Q_INVOKABLE void prepareGoogleSignIn() { auth.clear(); emit googleAuthChanged(); }
    Q_INVOKABLE void clearError() {}
    Q_INVOKABLE void cancelGoogleAuth() { busy=false; auth={{"phase","cancelled"}}; emit googleAuthChanged(); }
    Q_INVOKABLE bool accountCapability(const QString &,const QString &capability) { return capability=="mail_read"; }
    Q_INVOKABLE void connectGoogle(const QString &address,const QString &,bool,const QVariantMap &access) { email=address; requested=access; ++connections; busy=true; auth={{"phase","waiting_for_browser"}}; emit googleAuthChanged(); }
    Q_INVOKABLE void restoreGoogleAccount(const QString &,const QString &,double,bool,const QVariantMap &) { qFatal("ordinary Add cannot restore an archive"); }
    Q_INVOKABLE void updateAccountPreferences(const QString &id,const QString &name,const QString &signature,const QString &retention,const QVariantMap &sync,bool history) { analyzeHistory=history; require(sync.value("mail").toBool() && !sync.value("calendar").toBool() && sync.value("mail_access")=="read","chosen services and access are saved"); require(signature=="Original signature","connect preserves account signature"); savedRetention=retention; savedName=name; ++saves; preferencesStatus=saveSucceeds ? "Saved" : "Not saved — synthetic failure"; emit preferencesChanged(); emit accountPreferencesFinished(id,saveSucceeds); }
    Q_INVOKABLE void startGoogleSync(const QString &id,const QVariantMap &) { require(saveSucceeds&&!savedName.isEmpty(),"name saves before sync"); syncedAccount=id; ++syncStarts; }
signals:
    void googleAuthChanged(); void accountsChanged(); void connectedChanged(); void preferencesChanged();
    void accountPreferencesFinished(QString account,bool ok); void accountChanged();
    void assistanceReply(const QString &ticket,const QString &method,const QVariantMap &result,bool ok);
};
static QQuickItem *find(QQuickItem *root,const QString &name) { if(root->objectName()==name)return root; for(auto child:root->childItems())if(auto value=find(child,name))return value;return nullptr; }
int main(int argc,char **argv) {
    QGuiApplication app(argc,argv); QQuickStyle::setStyle("Basic");
    for(const auto file:{"Geist-Regular.ttf","Geist-Medium.ttf","Geist-SemiBold.ttf"})require(QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR)+"/fonts/"+file)>=0,"bundled fonts");
    QGuiApplication::setFont(QFont("Geist",10));
    QTemporaryDir temp; const auto path=temp.filePath("ConnectTest"); require(QDir().mkpath(path),"QML directory");
    QFile module(path+"/qmldir");require(module.open(QIODevice::WriteOnly),"module");module.write("module ConnectTest\n");
    for(const auto &file:QDir(QString(FM_GUI_SOURCE_DIR)).entryList({"*.qml"},QDir::Files)) {
        require(QFile::copy(QString(FM_GUI_SOURCE_DIR)+"/"+file,path+"/"+file),"copy production QML");const auto name=QFileInfo(file).completeBaseName();module.write((((name=="Theme" || name=="ShortcutRegistry")?"singleton ":"")+name+" 1.0 "+file+"\n").toUtf8());
    }
    module.close(); ConnectionBackend backend; QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings=false;
    QObject::connect(&qml,&QQmlEngine::warnings,[&](const auto &errors){warnings=true;for(const auto &error:errors)qWarning()<<error;});
    qml.addImportPath(temp.path());qml.rootContext()->setContextProperty("backend",&backend);
    qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport ConnectTest\nApplicationWindow { width:1000;height:720;visible:true;color:Theme.canvas; function appearance(theme,size) { Theme.themeId=theme;Theme.textSize=size } GoogleConnectionDialog { id:dialog; email:'person@example.test'; onSyncRequested: function(account) { backend.startGoogleSync(account,{}) } Component.onCompleted:open() } }");
    require(!qml.rootObjects().isEmpty(),"native connection dialog loads");auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());
    auto settle=[] { QCoreApplication::processEvents(); QThread::msleep(50); QCoreApplication::processEvents(); };settle();
    auto control=[&](const QString &name){auto item=find(window->contentItem(),name);require(item,"control exists");return item;};
    auto activate=[&](const QString &name){auto item=control(name);require(item->isVisible()&&item->isEnabled(),"control available");require(QMetaObject::invokeMethod(item,"clicked"),"actual click handler");settle();};
    auto dialog=window->findChild<QObject*>("googleSignInDialog");require(dialog,"connection popup");
    require(!find(window->contentItem(),"connectMeetAccess"),"no redundant Meetings permission");
    require(control("connectRetention")->property("currentIndex").toInt()==1,"retention defaults to one year");
    control("connectRetention")->setProperty("currentIndex",0);
    control("connectMailMode")->setProperty("currentIndex",0);control("connectCalendarAccess")->setProperty("checked",false);settle();
    const auto captures=qEnvironmentVariable("FM_CONNECTION_CAPTURES");
    auto capture=[&](const QString &name) { if(captures.isEmpty())return;require(QDir().mkpath(captures),"capture directory");const auto file=QDir(captures).filePath(name+".png");settle();require(!QFileInfo::exists(file)&&window->grabWindow().save(file),"retain connection frame"); };
    for(const auto theme:{"graphite","paper","terminal"}) { QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString("default")));capture(QString("access-")+theme); }
    if (!captures.isEmpty()) {
        window->resize(390,720); QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString("terminal")),Q_ARG(QVariant,QString("large")));capture("access-terminal-narrow-large");
        backend.configuration={{"available",false},{"error",QVariantMap{{"message","Google sign-in is not configured in this build. Install a build with the app registration, then try again."}}}};emit backend.googleAuthChanged();settle();
        capture("configuration-error-terminal-narrow-large");
        const auto action=control("beginGoogleSignIn");const auto position=action->mapToScene(QPointF(0,0));require(position.y()>=0&&position.y()+action->height()<=window->height(),"footer stays inside short connection window with an error");
        require(!action->isEnabled(),"unconfigured build cannot begin sign-in");
        const auto error=control("connectionError");require(error->isVisible() && error->mapToScene(QPointF(0,0)).y()+error->height()<=position.y(),"configuration error stays visible above disabled action");
        backend.configuration={{"available",true}};emit backend.googleAuthChanged();
        window->resize(1000,720); QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString("terminal")),Q_ARG(QVariant,QString("default")));settle();
    }
    activate("beginGoogleSignIn");require(backend.connections==1&&backend.email=="person@example.test"&&backend.requested==QVariantMap{{"mail","read"},{"calendar","none"}},"native access choices reach one sign-in request");
    require(dialog->property("step").toInt()==1&&backend.syncStarts==0,"browser step does not sync");capture("browser-terminal");
    backend.busy=false;backend.auth={{"phase","connected"},{"account","fixture-account"}};emit backend.googleAuthChanged();settle();
    require(dialog->property("step").toInt()==2&&backend.syncStarts==0,"successful sign-in waits for final action");control("connectedAccountName")->setProperty("text","My personal mail");capture("ready-terminal");
    const auto history=control("connectHistoryAnalysis");
    require(history->isVisible()&&history->property("currentIndex").toInt()==0,"a configured model offers history analysis, off by default");
    activate("finishGoogleConnection");require(backend.saves==1&&backend.syncStarts==0&&dialog->property("visible").toBool(),"failed display-name save retains form and starts no sync");
    require(!backend.analyzeHistory,"default leaves synced history for the Backlog");
    history->setProperty("currentIndex",1);settle();capture("ready-history-terminal");
    backend.saveSucceeds=true;activate("finishGoogleConnection");require(backend.saves==2&&backend.savedName=="My personal mail"&&backend.syncedAccount=="fixture-account"&&backend.syncStarts==1,"retry saves exact name and starts only the connected account");
    require(backend.savedRetention=="90_days","selected retention saves before sync");
    require(backend.analyzeHistory,"opting in analyzes the mail this sync downloads");
    require(backend.account=="fixture-account","the new account becomes current");
    require(!warnings,"no QML warnings");return 0;
}
#include "GoogleConnectionControlsTest.moc"
