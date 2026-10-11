// ReplyComposer UI test on a real MailBackend over the scripted engine: attaching
// files one at a time, edits during an import, exact retry, recipient suggestions,
// keyboard routing, multi-file drop, close protection, and spelling underlines
// and suggestions from a scripted spell checker.
#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include "SpellChecker.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QClipboard>
#include <QRegularExpression>
#include <QImage>
#include <QJsonArray>
#include <QJSValue>
#include <QKeyEvent>
#include <QElapsedTimer>
#include <QTextCursor>
#include <QTextDocument>
#include <QQuickTextDocument>
#include <QMimeData>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <functional>

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
// Knows two words and suggests one fix.
class ScriptedSpelling final : public SpellChecker {
public:
    void check(const QStringList &words, std::function<void(const QList<bool> &)> done) override { QList<bool> out; for (const auto &word : words) out << (word != "reply" && word != "Please"); done(out); }
    QStringList suggestions(const QString &word, int) override { return word == "Plese" ? QStringList{"Please", "Pulse"} : QStringList{}; }
    void correction(const QString &, std::function<void(const QString &)> done) override { done({}); }
    void learn(const QString &) override {}
    void ignore(const QString &) override {}
};
}

// The format bar: always shown, so selecting text moves nothing; bold,
// italic and underline for the selection or for what is typed next; bulleted
// and numbered lists that type on, end on an empty item, save and reload, and
// read "• " and "1. " in plain text.
static void formatting(QQuickWindow *window, QQuickItem *body, ReplyDocument *document) {
    const auto settle=[](int ms){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,5); };
    const auto key=[&](int code, Qt::KeyboardModifiers modifiers=Qt::NoModifier, const QString &text={}) {
        QKeyEvent press(QEvent::KeyPress,code,modifiers,text), release(QEvent::KeyRelease,code,modifiers,text);
        QCoreApplication::sendEvent(window,&press); QCoreApplication::sendEvent(window,&release); };
    const auto type=[&](const QString &text){ for (const auto c : text) key(0,Qt::NoModifier,QString(c)); settle(30); };
    const auto format=[&](const QString &name){ return document->formats().value(name).toBool(); };
    auto bar=find(window->contentItem(),"formatBar"), panel=find(window->contentItem(),"composer"), bold=find(window->contentItem(),"formatBold");
    require(bar && panel && bold,"format bar");
    document->load("",{}); body->forceActiveFocus(); settle(100);
    require(bar->isVisible(),"the format bar shows with nothing selected");
    type("Plain words here"); settle(100);
    const auto height=panel->height();
    QMetaObject::invokeMethod(body,"select",Q_ARG(int,0),Q_ARG(int,5)); settle(100);
    require(bar->isVisible() && panel->height()==height,"selecting text moves nothing");
    QMetaObject::invokeMethod(body,"select",Q_ARG(int,16),Q_ARG(int,16)); body->setProperty("cursorPosition",16); settle(50);
    // Bold with nothing selected: what is typed next is bold, and typing on stays so.
    key(Qt::Key_B,Qt::ControlModifier); settle(50);
    require(format("bold") && bold->property("selected").toBool(),"Bold lights up for the caret");
    type(" strong"); settle(100);
    QTextCursor check(document->textDocument()->textDocument());
    const auto boldAt=[&](int position){ check.setPosition(position); check.setPosition(position+1,QTextCursor::KeepAnchor); return check.charFormat().fontWeight()>=QFont::DemiBold; };
    require(!boldAt(10) && boldAt(17) && boldAt(22),"typed text takes the bold chosen with nothing selected");
    key(Qt::Key_B,Qt::ControlModifier); type(" calm"); settle(100);
    require(!boldAt(25) && !format("bold"),"bold turns off for what follows");
    body->setProperty("cursorPosition",5); settle(50);
    require(!format("bold"),"the caret in plain words shows no bold");
    // A bold word, in the saved HTML and back.
    require(document->html().contains("font-weight:700"),"bold saves in the HTML");
    // Bulleted list: two paragraphs become items, typing on adds one, an
    // empty item ends the list.
    document->load("First\nSecond",{}); body->forceActiveFocus();
    QMetaObject::invokeMethod(body,"select",Q_ARG(int,0),Q_ARG(int,12)); settle(50);
    key(Qt::Key_8,Qt::ControlModifier|Qt::ShiftModifier,"*"); settle(100);
    require(document->plainText()=="• First\n• Second" && document->html().contains("<ul>"),"⌘⇧8 makes the paragraphs a bulleted list");
    require(format("bullet") && find(window->contentItem(),"formatBullets")->property("selected").toBool(),"Bulleted list lights up");
    body->setProperty("cursorPosition",12); settle(30);
    key(Qt::Key_Return,Qt::NoModifier,"\r"); type("Third"); settle(100);
    require(document->plainText()=="• First\n• Second\n• Third","Return in a list starts the next item");
    key(Qt::Key_Return,Qt::NoModifier,"\r"); key(Qt::Key_Return,Qt::NoModifier,"\r"); type("After"); settle(100);
    require(document->plainText()=="• First\n• Second\n• Third\nAfter" && !format("bullet"),"Return on an empty item ends the list");
    // Saved and loaded again, the list is the same.
    const auto plain=document->plainText(), html=document->html();
    document->load("",{}); document->load(plain,html); settle(50);
    require(document->plainText()==plain && document->html()==html,"a list saves and loads unchanged");
    // Numbered: the list turns into numbers; again turns them back to paragraphs.
    QMetaObject::invokeMethod(body,"select",Q_ARG(int,0),Q_ARG(int,8)); settle(30);
    QMetaObject::invokeMethod(find(window->contentItem(),"formatNumbers"),"clicked"); settle(100);
    require(document->plainText()=="1. First\n2. Second\n• Third\nAfter","Numbered list numbers the chosen items");
    QMetaObject::invokeMethod(body,"select",Q_ARG(int,0),Q_ARG(int,8)); settle(30);
    QMetaObject::invokeMethod(find(window->contentItem(),"formatNumbers"),"clicked"); settle(100);
    require(document->plainText()=="First\nSecond\n• Third\nAfter","Numbered list again turns them back to paragraphs");
    document->undo(); settle(50);
    require(document->plainText()=="1. First\n2. Second\n• Third\nAfter","undo brings a list change back in one step");
    // Backspace at the start of an item takes it out of the list, keeping its words.
    body->setProperty("cursorPosition",6); settle(30);
    key(Qt::Key_Backspace); settle(100);
    require(document->plainText()=="1. First\nSecond\n• Third\nAfter","Backspace at an item's start ends its list item");
    qInfo()<<"Format bar: always shown, typing styles, lists that type on and save";
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
    for (const auto name : {"FmScrollBar.qml","Theme.qml","FmDialog.qml","FmConfirmDialog.qml","FmButton.qml","FmToolTip.qml","FmIcon.qml","RecipientField.qml","ReplyComposer.qml","FmMenu.qml","FmMenuItem.qml","TextMenu.qml","TextMenuHandler.qml"})
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
    const QJsonArray candidates{QJsonObject{{"name","Zoë Current"},{"email","zoe@example.test"},{"account","fixture-account"},{"account_name","Work"},{"sent",1234},{"received",3}},QJsonObject{{"name","Zoë Elsewhere"},{"email","elsewhere@example.test"},{"account","other"},{"account_name","Personal"},{"sent",0},{"received",0}}};
    lookups[0].peer->write(reply(lookups[0].id,{{"account","fixture-account"},{"query","zo"},{"suggestions",candidates}}));
    await([&]{return find(window->contentItem(),"recipientSuggestion_to_recipients_1");},"actual suggestion rows appear");
    {
        // Mail each way sits beside the address, short; a row with none shows no counts.
        std::function<void(QQuickItem *, QStringList &)> shown=[&](QQuickItem *item, QStringList &texts) {
            if (!item->isVisible()) return;
            if (item->inherits("QQuickText")) texts << item->property("text").toString();
            for (auto child : item->childItems()) shown(child, texts);
        };
        auto counts=find(window->contentItem(),"recipientCounts_to_recipients_0"); QStringList texts;
        require(counts && counts->isVisible(),"counts shown for a correspondent");
        shown(counts,texts);
        require(texts==QStringList({"1.2k","3"}),"sent then received, compact");
        auto none=find(window->contentItem(),"recipientCounts_to_recipients_1");
        require(none && !none->isVisible(),"no counts for an address with no mail");
    }
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
    // Typing on narrows the rows already shown at once, before the new lookup answers.
    type("an"); await([&]{return lookups.size()==4;},"narrowing lookup");
    lookups[3].peer->write(reply(lookups[3].id,{{"account","fixture-account"},{"query","an"},{"suggestions",QJsonArray{
        QJsonObject{{"name","Ana Narrow"},{"email","ana@example.test"},{"account","fixture-account"},{"account_name","Work"}},
        QJsonObject{{"name","Anatoly Wide"},{"email","wide@example.test"},{"account","fixture-account"},{"account_name","Work"}}}}}));
    await([&]{return find(window->contentItem(),"recipientSuggestion_to_recipients_1");},"two candidates shown");
    type("ana@");
    QCoreApplication::processEvents();
    {
        auto first=find(window->contentItem(),"recipientSuggestion_to_recipients_0");
        QVariant row=first ? first->property("modelData") : QVariant();
        if (row.canConvert<QJSValue>()) row=row.value<QJSValue>().toVariant();
        require(lookups.size()==4 && first && first->isVisible() && row.toMap()["email"]=="ana@example.test"
            && !find(window->contentItem(),"recipientSuggestion_to_recipients_1"),"typing keeps matching rows and drops the rest without waiting");
    }
    await([&]{return lookups.size()==5;},"lookup for the longer text");
    lookups[4].peer->write(reply(lookups[4].id,{{"account","fixture-account"},{"query","ana@"},{"suggestions",QJsonArray{}}}));
    press(Qt::Key_Escape); type("");
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

    // Paste: a screenshot on the clipboard becomes a named PNG attachment;
    // a second paste while it imports joins the queue. Only on the offscreen
    // platform, whose clipboard is private to this process.
    if (QGuiApplication::platformName()=="offscreen") {
        auto document=window->findChild<ReplyDocument*>("replyDocument"); require(document,"composer document for paste");
        auto editor=find(window->contentItem(),"replyComposer"); require(editor,"composer body for paste");
        const auto textBefore=document->plainText();
        const auto pasteKey=[&]{ editor->forceActiveFocus(); QKeyEvent paste(QEvent::KeyPress,Qt::Key_V,Qt::ControlModifier); QCoreApplication::sendEvent(editor,&paste); };
        QImage shot(40,30,QImage::Format_RGB32); shot.fill(Qt::red);
        QGuiApplication::clipboard()->setImage(shot);
        require(document->clipboardAttachable(),"a copied screenshot can be attached");
        pasteKey();
        await([&]{return imports.size()==6;},"pasted screenshot starts an import");
        const auto pastedPath=imports[5].params["path"].toString();
        static const QRegularExpression screenshotName("^Screenshot \\d{4}-\\d{2}-\\d{2} at \\d{2}\\.\\d{2}\\.\\d{2}( \\(\\d+\\))?\\.png$");
        require(screenshotName.match(QFileInfo(pastedPath).fileName()).hasMatch(),qPrintable("pasted screenshot is named like a screenshot: "+pastedPath));
        QImage written(pastedPath); require(written.size()==QSize(40,30) && written.pixelColor(5,5)==QColor(Qt::red),"the attachment is the pasted image as PNG");
        require(document->plainText()==textBefore,"pasting a screenshot leaves the text alone");
        QImage second(20,20,QImage::Format_RGB32); second.fill(Qt::blue);
        QGuiApplication::clipboard()->setImage(second);
        pasteKey();
        await([&]{return backend.attachmentImport()["queued"].toInt()==1;},"a second paste joins the running import");
        succeed(5,"pasted-1");
        await([&]{return imports.size()==7;},"the second screenshot imports next");
        const auto secondPath=imports[6].params["path"].toString();
        require(secondPath!=pastedPath && screenshotName.match(QFileInfo(secondPath).fileName()).hasMatch(),"each paste gets its own screenshot name");
        succeed(6,"pasted-2"); await([&]{return !backend.attachmentImportPending() && !backend.draftBusy();},"pasted screenshots attached");
        // Copied files attach; an image that comes with text pastes the text.
        QFile copied(fixture.temporary.filePath("copied.txt")); require(copied.open(QIODevice::WriteOnly) && copied.write("copied") > 0,"copied file"); copied.close();
        auto files=new QMimeData; files->setUrls({file("copied.txt")}); files->setText("copied.txt");
        QGuiApplication::clipboard()->setMimeData(files);
        pasteKey();
        await([&]{return imports.size()==8;},"copied file starts an import");
        require(imports[7].params["path"].toString()==fixture.temporary.filePath("copied.txt") && document->plainText()==textBefore,"copied files attach instead of pasting their names");
        succeed(7,"copied"); await([&]{return !backend.attachmentImportPending() && !backend.draftBusy();},"copied file attached");
        auto mixed=new QMimeData; mixed->setImageData(shot); mixed->setText("Cell text");
        QGuiApplication::clipboard()->setMimeData(mixed);
        require(!document->clipboardAttachable(),"an image with text is pasted as text");
        pasteKey();
        await([&]{return document->plainText().contains("Cell text");},"image with text pastes the text");
        require(imports.size()==8,"no attachment from an image that came with text");
        QGuiApplication::clipboard()->clear();
    } else qWarning()<<"Paste checks skipped: they run only on the offscreen platform.";

    // Spelling: a red dotted rule under a misspelled word, and its suggestions
    // first in the editor's menu; choosing one replaces the word undoably.
    auto spelling=window->findChild<ReplyDocument*>("replyDocument"); require(spelling,"composer document");
    auto body=find(window->contentItem(),"replyComposer"); require(body,"composer body");
    ScriptedSpelling checker; spelling->setSpellChecker(&checker);
    const auto bodyBefore=spelling->plainText();
    spelling->load("Plese reply",{});
    await([&]{return spelling->misspellings().size()==1;},"misspelled word published to the composer");
    QRectF wordStart,wordEnd;
    QMetaObject::invokeMethod(body,"positionToRectangle",Q_RETURN_ARG(QRectF,wordStart),Q_ARG(int,0));
    QMetaObject::invokeMethod(body,"positionToRectangle",Q_RETURN_ARG(QRectF,wordEnd),Q_ARG(int,5));
    const auto ruled=[&](qreal from,qreal to) {
        const auto frame=window->grabWindow(); const qreal scale=frame.devicePixelRatio();
        const auto a=body->mapToScene(QPointF(from,wordStart.bottom()-4)), b=body->mapToScene(QPointF(to,wordStart.bottom()+2));
        int red=0;
        for(int y=int(a.y()*scale);y<int(b.y()*scale);++y) for(int x=int(a.x()*scale);x<int(b.x()*scale);++x) {
            const auto c=frame.pixelColor(x,y); if(c.red()-c.green()>50 && c.red()-c.blue()>40) ++red;
        }
        return red;
    };
    QRectF replyStart,replyEnd;
    QMetaObject::invokeMethod(body,"positionToRectangle",Q_RETURN_ARG(QRectF,replyStart),Q_ARG(int,6));
    QMetaObject::invokeMethod(body,"positionToRectangle",Q_RETURN_ARG(QRectF,replyEnd),Q_ARG(int,11));
    require(ruled(wordStart.x(),wordEnd.x())>4,"the misspelled word has a red rule under it");
    require(ruled(replyStart.x()+1,replyEnd.x())==0,"a correct word has none");
    const auto point=body->mapToScene(QPointF((wordStart.x()+wordEnd.x())/2,wordStart.center().y()));
    const auto global=window->mapToGlobal(point.toPoint());
    QMouseEvent rightDown(QEvent::MouseButtonPress,point,global,Qt::RightButton,Qt::RightButton,Qt::NoModifier);
    QMouseEvent rightUp(QEvent::MouseButtonRelease,point,global,Qt::RightButton,Qt::NoButton,Qt::NoModifier);
    QCoreApplication::sendEvent(window,&rightDown); QCoreApplication::sendEvent(window,&rightUp);
    auto root=window->contentItem()->parentItem() ? window->contentItem()->parentItem() : window->contentItem();
    await([&]{auto item=find(root,"spellingSuggestion_0");return item && item->isVisible();},"suggestions open with the menu");
    auto first=find(root,"spellingSuggestion_0"), second=find(root,"spellingSuggestion_1");
    if(!captures.isEmpty()) { QCoreApplication::processEvents(); require(window->grabWindow().save(captures+"/composer-spelling.png"),"spelling capture"); }
    require(first->property("text")=="Please" && second && second->property("text")=="Pulse","suggestions listed in the checker's order");
    auto menuRows=first->parentItem()->parentItem();
    auto undoRow=find(menuRows,"textMenuUndo");
    require(undoRow,"the usual edit rows");
    await([&]{return undoRow->property("separatorAbove").toBool();},"the usual edit rows follow the suggestions");
    require(first->mapToScene(QPointF()).y()<undoRow->mapToScene(QPointF()).y(),"suggestions come first");
    require(QMetaObject::invokeMethod(first,"clicked"),"choose a suggestion");
    require(spelling->plainText()=="Please reply","the suggestion replaces the word");
    await([&]{return spelling->misspellings().isEmpty();},"the replaced word loses its rule");
    require(!find(window->contentItem(),"composer")->property("popupOpen").toBool(),"choosing closes the menu");
    spelling->undo();
    require(spelling->plainText()=="Plese reply","undo brings the word back");
    QCoreApplication::sendEvent(window,&rightDown); QCoreApplication::sendEvent(window,&rightUp);
    await([&]{auto item=find(menuRows,"spellingIgnore");return item && item->isVisible();},"menu reopens");
    require(QMetaObject::invokeMethod(find(menuRows,"spellingIgnore"),"clicked"),"ignore the word");
    await([&]{return spelling->misspellings().isEmpty();},"an ignored word loses its rule");
    const auto correct=body->mapToScene(QPointF((replyStart.x()+replyEnd.x())/2,replyStart.center().y()));
    QMouseEvent plainDown(QEvent::MouseButtonPress,correct,window->mapToGlobal(correct.toPoint()),Qt::RightButton,Qt::RightButton,Qt::NoModifier);
    QMouseEvent plainUp(QEvent::MouseButtonRelease,correct,window->mapToGlobal(correct.toPoint()),Qt::RightButton,Qt::NoButton,Qt::NoModifier);
    QCoreApplication::sendEvent(window,&plainDown); QCoreApplication::sendEvent(window,&plainUp);
    await([&]{return undoRow->isVisible();},"menu over a correct word");
    require(!find(menuRows,"spellingSuggestion_0"),"a correct word offers no suggestions");
    require(!find(menuRows,"spellingIgnore")->isVisible(),"and no spelling rows");
    await([&]{return !undoRow->property("separatorAbove").toBool();},"the menu starts with Undo again");
    QKeyEvent closeMenu(QEvent::KeyPress,Qt::Key_Escape,Qt::NoModifier); QCoreApplication::sendEvent(window,&closeMenu);
    // A short text replacing one marked far along: the underlines catch up a
    // moment later and never ask the editor about places past its end.
    static QStringList rangeWarnings;
    static QtMessageHandler previousHandler = nullptr;
    previousHandler = qInstallMessageHandler([](QtMsgType type, const QMessageLogContext &context, const QString &message) {
        if (message.contains("out of range")) rangeWarnings << message;
        if (previousHandler) previousHandler(type, context, message);
    });
    QStringList words; for (int i=0;i<60;++i) words<<"reply";
    spelling->load(words.join(' ')+" Wrongg reply",{});
    await([&]{return spelling->misspellings().size()==1 && spelling->misspellings().first().toMap()["start"].toInt()>300;},"a misspelled word far along is marked");
    spelling->load("reply",{});
    await([&]{return spelling->misspellings().isEmpty();},"the short text has no marks");
    QElapsedTimer drawn; drawn.start(); while(drawn.elapsed()<200) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
    qInstallMessageHandler(previousHandler);
    if(!rangeWarnings.isEmpty()) { qWarning()<<rangeWarnings.mid(0,4); require(false,"spelling underlines never reach past the text"); }
    spelling->setSpellChecker(nullptr);
    formatting(window, body, spelling);
    spelling->load(bodyBefore,{});
    require(!warnings,"production QML has no warnings");
    qInfo()<<"Composer input passed: serial files, in-flight edits, exact uncertain replay, navigation, close protection, cached suggestions, keyboard routing, native multi-file drop and spelling";
}
};
int main(int argc,char **argv) { QGuiApplication app(argc,argv); QQuickStyle::setStyle("Basic"); ComposerInputTest::run(); return 0; }
