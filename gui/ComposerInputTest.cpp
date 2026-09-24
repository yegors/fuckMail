#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QJsonArray>
#include <QJSValue>
#include <QKeyEvent>
#include <QMimeData>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>

using namespace GoogleOperationFixture;
namespace {
QQuickItem *find(QQuickItem *item, const QString &name) {
    if (item->objectName() == name) return item;
    for (auto child : item->childItems()) if (auto found = find(child, name)) return found;
    return nullptr;
}
QJsonObject initialDraft(QString id = "draft-fixture") {
    return {{"account","fixture-account"},{"draft_id",id},{"mode","new"},{"revision",1},
        {"body","Original words"},{"body_html",QJsonValue::Null},{"subject","Composer fixture"},
        {"from","me@example.test"},{"from_name","Me"},{"discarded",false},{"signature_text",""},
        {"to_recipients",QJsonArray{}},{"cc_recipients",QJsonArray{}},{"bcc_recipients",QJsonArray{}},
        {"attachment_ids",QJsonArray{}},{"attachments",QJsonArray{}}};
}
struct Held { QPointer<QLocalSocket> peer; QString id; QJsonObject params; };
}

class ComposerInputTest {
public:
static void run() {
    for (const auto weight : {"Regular", "Medium", "SemiBold"})
        require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR)+"/fonts/Geist-"+weight+".ttf") >= 0,"bundled fonts");
    Engine fixture;
    MailBackend backend(fixture.profile,"unused-fixture-engine",nullptr,false,true);
    backend.m_connected = true; backend.m_account = "fixture-account";
    backend.m_enginePid = QCoreApplication::applicationPid(); backend.m_activeDraft = "draft-fixture";
    QJsonObject saved = initialDraft(); backend.m_draft = saved.toVariantMap();
    backend.m_allAccounts = QVariantList{QVariantMap{{"id","fixture-account"},{"name","Work"},{"email","me@example.test"},{"lifecycle",QVariantMap{{"state","active"},{"revision",1}}}}};
    require(backend.publishAccounts(backend.m_allAccounts),"active account");
    QObject::disconnect(&backend.m_socket,SIGNAL(connected()),&backend,nullptr);
    backend.m_socket.connectToServer(fixture.server.serverName());
    await([&]{return backend.m_socket.state()==QLocalSocket::ConnectedState;},"cached socket");
    bool hello=false;
    backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto, bool ok){hello=ok;});
    await([&]{return hello;},"cached hello");
    QList<Held> imports, lookups;
    int saves=0;
    fixture.customRequest = [&](QLocalSocket *peer,const QJsonObject &request) {
        const auto method=request["method"].toString(), id=request["id"].toString();
        const auto params=request["params"].toObject();
        if (method=="draft_attachment_import") { imports.append({peer,id,params}); return true; }
        if (method=="recipient_suggestions") { lookups.append({peer,id,params}); return true; }
        if (method=="draft_save") {
            require(params["expected_revision"]==saved["revision"],"save follows import revision");
            for (const auto key : {"body","body_html","subject","signature_text","to_recipients","cc_recipients","bcc_recipients","attachment_ids"}) saved[key]=params[key];
            saved["revision"]=saved["revision"].toInteger()+1; ++saves;
            peer->write(reply(id,saved)); return true;
        }
        if (method=="accounts") peer->write(reply(id,{{"accounts",QJsonArray::fromVariantList(backend.m_allAccounts)}}));
        else peer->write(reply(id,{}));
        return true;
    };
    const auto succeed = [&](int index, const QString &fileId) {
        const auto held=imports[index];
        require(held.params["draft_id"]==saved["draft_id"] && held.params["expected_revision"]==saved["revision"],"captured draft and serial revision");
        const QJsonObject file{{"account","fixture-account"},{"id",fileId},{"name",fileId+".txt"},{"state","available"},{"size",27}};
        auto ids=saved["attachment_ids"].toArray(); ids.append(fileId); saved["attachment_ids"]=ids;
        auto files=saved["attachments"].toArray(); files.append(file); saved["attachments"]=files;
        saved["revision"]=saved["revision"].toInteger()+1;
        held.peer->write(reply(held.id,{{"draft",saved},{"attachment",file}}));
    };
    const auto failImport = [&](int index, const QString &code) {
        const auto held=imports[index];
        held.peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",held.id},{"ok",false},{"error",QJsonObject{{"code",code},{"message","Fixture interruption"}}}}).toJson(QJsonDocument::Compact)+'\n');
    };
    auto file=[&](QString name){return QUrl::fromLocalFile(fixture.temporary.filePath(name));};
    backend.importDraftAttachments({file("one.txt"),file("two.txt")},"fixture-account","draft-fixture");
    await([&]{return imports.size()==1;},"first file starts");
    require(backend.attachmentImport()["queued"].toInt()==1,"second waits");
    backend.editDraft("Words typed while attaching"); backend.m_draftTimer.stop();
    succeed(0,"one");
    await([&]{return imports.size()==2;},"second starts after saving newer text");
    require(saves==1 && saved["body"]=="Words typed while attaching","authored edits preserved between files");
    const auto intent=imports[1].params;
    failImport(1,"timeout");
    await([&]{return backend.attachmentImport()["retry"].toBool();},"unknown outcome retained");
    backend.retryDraftAttachment();
    await([&]{return imports.size()==3;},"retry dispatched");
    require(imports[2].params==intent,"retry preserves exact intent and revision");
    succeed(2,"two"); await([&]{return !backend.attachmentImportPending();},"both files finish");
    require(backend.m_draft["attachment_ids"].toList().size()==2,"both selections retained");
    backend.importDraftAttachments({file("three.txt"),file("four.txt")},"fixture-account","draft-fixture");
    await([&]{return imports.size()==4;},"next queue begins");
    backend.acceptDraft(initialDraft("different-draft"),false);
    succeed(3,"three"); await([&]{return !backend.attachmentImportBusy();},"original import finishes after navigation");
    require(backend.m_draft["draft_id"]=="different-draft" && backend.m_draft["attachment_ids"].toList().isEmpty(),"late receipt cannot replace another draft");
    require(imports.size()==4,"unstarted files pause on navigation");
    backend.acceptDraft(saved,false);
    require(backend.attachmentImport()["can_resume"].toBool(),"waiting files visible when returning");
    auto changedAccount=backend.m_allAccounts[0].toMap();
    changedAccount["lifecycle"]=QVariantMap{{"state","active"},{"revision",2}}; backend.m_allAccounts[0]=changedAccount;
    backend.retryDraftAttachment();
    require(imports.size()==4 && backend.attachmentImport()["status"].toString().contains("account changed"),"remove/restore lifecycle fences unstarted files");
    backend.shutdown(); require(!backend.m_shutdown,"queued file selection is not silently lost on close");
    backend.skipQueuedDraftAttachments(); require(!backend.attachmentImportPending(),"skip only unstarted files");

    backend.m_allAccounts.append(QVariantMap{{"id","other"},{"name","Personal"},{"email","personal@example.test"},{"lifecycle",QVariantMap{{"state","active"},{"revision",1}}}});
    require(backend.publishAccounts(backend.m_allAccounts),"multiple From choices");
    QTemporaryDir copy; const auto modulePath=copy.filePath("ComposerTest"); require(QDir().mkpath(modulePath),"QML module");
    for (const auto name : {"FmScrollBar.qml","Theme.qml","FmDialog.qml","FmConfirmDialog.qml","FmButton.qml","FmToolTip.qml","FmIcon.qml","RecipientField.qml","ReplyComposer.qml"})
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name),"production QML copy");
    QFile module(modulePath+"/qmldir"); require(module.open(QIODevice::WriteOnly),"module registration");
    module.write("module ComposerTest\nsingleton Theme 1.0 Theme.qml\nReplyComposer 1.0 ReplyComposer.qml\nRecipientField 1.0 RecipientField.qml\n"); module.close();
    qmlRegisterType<ReplyDocument>("ComposerTest",1,0,"ReplyDocument");
    QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings=false;
    QObject::connect(&qml,&QQmlEngine::warnings,[&](const QList<QQmlError> &errors){warnings=true; for(const auto &error:errors) qWarning()<<error.toString();});
    qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&backend);
    qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import ComposerTest 1.0
ApplicationWindow { width: 780; height: 900; visible: true; color: Theme.bg
 function appearance(theme, size) { Theme.themeId=theme; Theme.textSize=size }
 ReplyComposer { objectName: "composer"; width: parent.width - 40; x: 20; anchors.top: parent.top; anchors.topMargin: 20 }
})QML",QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(),"composer loads");
    auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first()); require(window,"window");
    await([&]{return window->isExposed();},"composer window exposed before keyboard input");
    window->requestActivate();
    await([&]{return window->isActive();},"composer window active before keyboard input");
    auto bodyScroll=find(window->contentItem(),"replyBodyScroll");
    auto bodyBar=bodyScroll ? find(bodyScroll,"persistentScrollBar") : nullptr;
    require(bodyBar && bodyBar->width() >= 14,"composer scrollbar keeps a wide hit target");
    require(!bodyBar->isVisible(),"composer hides the scrollbar until the body overflows");
    const auto barEdge=bodyBar->mapToScene(QPointF(bodyBar->width(),0));
    const auto scrollEdge=bodyScroll->mapToScene(QPointF(bodyScroll->width(),0));
    require(qAbs(barEdge.x()-scrollEdge.x())<1 && qAbs(bodyBar->height()-bodyScroll->height())<1,
        "composer scrollbar fills the right edge instead of covering the editor's first column");
    auto from=find(window->contentItem(),"draftFromAccount"); require(from && from->isEnabled(),"From account selector enabled");
    require(QMetaObject::invokeMethod(from,"clicked"),"open real From menu");
    await([&]{return find(window->contentItem(),"composer")->property("popupOpen").toBool();},"From menu blocks workspace shortcuts");
    const auto fromCaptures=qEnvironmentVariable("FM_COMPOSER_SCREENSHOTS");
    if(!fromCaptures.isEmpty()) {
        QDir().mkpath(fromCaptures);
        for(const auto theme:{"graphite","paper","terminal"}) {
            QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString("default")));
            for(int frame=0;frame<2;++frame) {
                QEventLoop paint; const auto next=QObject::connect(window,&QQuickWindow::frameSwapped,&paint,&QEventLoop::quit);
                QTimer::singleShot(500,&paint,&QEventLoop::quit); window->requestUpdate(); paint.exec(); QObject::disconnect(next);
            }
            require(window->grabWindow().save(fromCaptures+"/from-"+theme+".png"),"From menu native capture");
        }
    }
    QKeyEvent closeFrom(QEvent::KeyPress,Qt::Key_Escape,Qt::NoModifier); QCoreApplication::sendEvent(window,&closeFrom);
    await([&]{return !find(window->contentItem(),"composer")->property("popupOpen").toBool();},"Escape closes From menu");
    auto entry=find(window->contentItem(),"draftRecipient_to_recipients"); require(entry,"recipient entry");
    auto type=[&](const QString &text) { entry->forceActiveFocus(); entry->setProperty("text",text); require(QMetaObject::invokeMethod(entry,"textEdited"),"real text editing handler"); };
    auto press=[&](int key){QKeyEvent event(QEvent::KeyPress,key,Qt::NoModifier); QCoreApplication::sendEvent(window,&event);};
    type("zo"); await([&]{return lookups.size()==1;},"debounced cached lookup");
    const QJsonArray candidates{QJsonObject{{"name","Zoë Current"},{"email","zoe@example.test"},{"account","fixture-account"},{"account_name","Work"}},QJsonObject{{"name","Zoë Elsewhere"},{"email","elsewhere@example.test"},{"account","other"},{"account_name","Personal"}}};
    lookups[0].peer->write(reply(lookups[0].id,{{"account","fixture-account"},{"query","zo"},{"suggestions",candidates}}));
    await([&]{return find(window->contentItem(),"recipientSuggestion_to_recipients_1");},"actual suggestion rows appear");
    const auto captures=qEnvironmentVariable("FM_COMPOSER_SCREENSHOTS");
    if(!captures.isEmpty()) {
        QDir().mkpath(captures);
        for(const auto theme:{"graphite","paper","terminal"}) {
            require(QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString("default"))),"theme selector");
            QCoreApplication::processEvents(); require(window->grabWindow().save(captures+"/composer-"+theme+".png"),"native capture");
        }
        window->setWidth(520);
        QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString("paper")),Q_ARG(QVariant,QString("large")));
        QCoreApplication::processEvents(); require(window->grabWindow().save(captures+"/composer-paper-large.png"),"narrow large text capture");
        auto suggestion=find(window->contentItem(),"recipientSuggestion_to_recipients_1");
        require(suggestion && suggestion->mapToScene(QPointF(suggestion->width(),0)).x() < window->width(),"suggestions stay inside resized window");
        window->setWidth(780);
    }
    press(Qt::Key_Down); press(Qt::Key_Return);
    require(backend.m_draft["to_recipients"].toList().value(0).toMap()["email"]=="elsewhere@example.test","arrow and Enter choose exact address");
    type("zo"); await([&]{return lookups.size()==2;},"second lookup");
    require(lookups[1].params["exclude"].toArray().contains("elsewhere@example.test"),"existing recipient excluded");
    press(Qt::Key_Escape);
    type("fresh@example.test"); press(Qt::Key_Tab);
    require(backend.m_draft["to_recipients"].toList().size()==2,"raw address accepted with Tab");
    lookups[1].peer->write(reply(lookups[1].id,{{"account","fixture-account"},{"query","zo"},{"suggestions",candidates}}));
    QCoreApplication::processEvents();
    require(!find(window->contentItem(),"composer")->property("popupOpen").toBool(),"late result cannot reopen stale suggestions");
    type("mouse"); await([&]{return lookups.size()==3;},"mouse choice lookup");
    const QString quotedName="Zoë, \"Design\"";
    lookups[2].peer->write(reply(lookups[2].id,{{"account","fixture-account"},{"query","mouse"},{"suggestions",QJsonArray{QJsonObject{{"name",quotedName},{"email","mouse@example.test"},{"account","fixture-account"},{"account_name","Work"}}}}}));
    await([&]{auto item=find(window->contentItem(),"recipientSuggestion_to_recipients_0");return item && item->isVisible();},"mouse candidate visible");
    QCoreApplication::processEvents();
    auto mouseChoice=find(window->contentItem(),"recipientSuggestion_to_recipients_0");
    const auto mousePosition=mouseChoice->mapToScene(QPointF(mouseChoice->width()/2,mouseChoice->height()/2));
    const auto globalPosition=window->mapToGlobal(mousePosition.toPoint());
    QMouseEvent mouseDown(QEvent::MouseButtonPress,mousePosition,globalPosition,Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
    QMouseEvent mouseUp(QEvent::MouseButtonRelease,mousePosition,globalPosition,Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
    QCoreApplication::sendEvent(window,&mouseDown); QCoreApplication::sendEvent(window,&mouseUp);
    require(backend.m_draft["to_recipients"].toList().value(2).toMap()["name"]==quotedName,"mouse selection preserves quoted display name without parsing a formatted address");
    backend.m_draftTimer.stop(); backend.saveDraft(); await([&]{return !backend.m_draftDirty && !backend.draftBusy();},"recipient edits saved");
    auto composer=find(window->contentItem(),"composer");
    QMimeData mime; mime.setUrls({file("dropped-a.txt"),file("dropped-b.txt")});
    const auto position=composer->mapToScene(QPointF(200,200));
    QDragEnterEvent enter(position.toPoint(),Qt::CopyAction,&mime,Qt::LeftButton,Qt::NoModifier);
    QCoreApplication::sendEvent(window,&enter); require(enter.isAccepted(),"native file drag accepted");
    QDropEvent drop(position,Qt::CopyAction,&mime,Qt::LeftButton,Qt::NoModifier);
    QCoreApplication::sendEvent(window,&drop); require(drop.isAccepted(),"native file drop accepted");
    await([&]{return imports.size()==5;},"drop starts same import pipeline");
    require(backend.attachmentImport()["queued"].toInt()==1,"drop queues both files");
    failImport(4,"invalid_request"); await([&]{return !backend.attachmentImportBusy();},"definite failure completes");
    require(backend.attachmentImport()["can_resume"].toBool(),"remaining files survive failed file");
    backend.skipQueuedDraftAttachments();
    backend.importDraftAttachments({QUrl("https://example.test/file")},"fixture-account","draft-fixture");
    require(!backend.attachmentImportPending() && imports.size()==5,"remote URL cannot become a file import");
    require(!warnings,"production QML has no warnings");
    qInfo()<<"Composer input passed: serial files, in-flight edits, exact uncertain replay, navigation, close protection, cached suggestions, keyboard routing and native multi-file drop";
}
};
int main(int argc,char **argv) { QGuiApplication app(argc,argv); QQuickStyle::setStyle("Basic"); ComposerInputTest::run(); return 0; }
