#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontInfo>
#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QQuickItem>
#include <QMouseEvent>
#include <QKeyEvent>
using namespace GoogleOperationFixture;

class MailSelectionTest {
    struct Held { QPointer<QLocalSocket> peer; QJsonObject request; };
    struct Harness {
        Engine engine;
        MailBackend backend{engine.profile,"unused"};
        QList<Held> writes;
        QStringList methods;
        QJsonObject lastQuery;
        QVariantList rows;
        Harness() {
            backend.m_account = "personal";
            backend.m_syntheticProfile = false;
            backend.m_preferences = {{"notifications","off"},{"theme","graphite"},{"text_size","default"}};
            backend.m_allAccounts = {QVariantMap{{"id","personal"},{"name","Personal"},{"email","test@example.test"},
                {"conversation_count",3},{"lifecycle",QVariantMap{{"state","active"},{"revision",1}}},
                {"google",QVariantMap{{"connection_state","connected"},{"capabilities",QVariantMap{{"mail_read",true},{"mail_write",true}}}}}}};
            backend.publishAccounts(backend.m_allAccounts);
            for (const auto &id : {"a","b","c"}) rows.append(QVariantMap{{"id",id},{"subject",QString("Labeled message ")+id},
                {"sender_name","Sender"},{"sender_email","sender@example.test"},{"latest_at","2026-09-14T10:00:00Z"},
                {"message_ids",QVariantList{QString(id)+"-original"}},{"revision",3},{"message_count",1},
                {"categories",QVariantList{QVariantMap{{"id","imported"},{"name","Imported folder"}}}},
                {"group","review"},{"kind","unknown"},{"unread",true},{"starred",false}});
            backend.publishConversations(rows);
            backend.m_querySummary = {{"filtered_conversation_count",3},{"filtered_group_counts",QVariantMap{{"review",3}}}};
            engine.customRequest = [this](auto *peer, const auto &request) {
                const auto method = request["method"].toString(); methods.append(method);
                if (method == "flag" || method == "set_category" || method == "mail_filing" || method == "draft_discard") writes.append({peer,request});
                else {
                    QJsonObject result;
                    if (method == "accounts") result["accounts"] = QJsonArray::fromVariantList(backend.m_allAccounts);
                    else if (method == "conversations") { lastQuery = request["params"].toObject(); result = {{"conversations",QJsonArray::fromVariantList(rows)},{"revision",3},{"filtered_conversation_count",3},{"folder_conversation_count",3},{"next_offset",QJsonValue::Null}}; }
                    else if (method == "assist_results") result={{"items",QJsonArray{QJsonObject{{"task","summary"},{"target","a-original"},{"stale",false},{"partial",false},{"source_ids",QJsonArray{"a-original"}},{"schema",2},{"output",QJsonObject{{"summary","You granted Example Mail access to your account data. Review access if you did not authorize it."},{"source_ids",QJsonArray{"a-original"}}}},{"model",QJsonObject{{"name","gemma4:e4b"}}}}}}};
                    else if (method == "assist_settings_get") result={{"revision",0},{"settings",QJsonObject{{"accounts",QJsonObject{}},{"model",QJsonValue::Null}}}};
                    else if (method == "message_source") {
                        const auto params = request["params"].toObject();
                        result = {{"account","personal"},{"id",params["id"]},{"source_version",params["source_version"]},{"raw","From: Sender <sender@example.test>\r\nTo: Me <me@example.test>\r\n\r\nRetained body"},{"body_html","<p><b>Retained HTML</b><img src=\"https://images.example.test/pixel\" alt=\"Logo\"></p>"},{"metadata",QJsonArray{QJsonObject{{"label","Cc"},{"people",QJsonArray{QJsonObject{{"name","Team"},{"email","team@example.test"}}}}}}}};
                    }
                    else if (method == "categories") result["categories"] = QJsonArray{QJsonObject{{"id","imported"},{"name","Imported folder"},{"conversation_count",3}}};
                    else if (method == "view_save") result["revision"] = 1;
                    else if (method == "draft_get") result = {{"conversation_id",request["params"].toObject()["conversation_id"]},{"account","personal"},{"revision",0}};
                    else if (method == "conversation") {
                        const auto id = request["params"].toObject()["id"].toString();
                        result = {{"id",id},{"revision",3},{"message_ids",QJsonArray{id+"-original"}},{"messages",QJsonArray{QJsonObject{{"id",id+"-original"},{"unread",true},{"starred",false},{"sender_name","Sender"},{"sent_at","2026-09-14T10:00:00Z"},{"text","Retained plain body"},{"source_available",true},{"source_version",QString(64,char(97))},{"sent",false}}}}};
                    }
                    peer->write(reply(request["id"].toString(),result));
                }
                return true;
            };
            QObject::disconnect(&backend.m_socket,SIGNAL(connected()),&backend,nullptr);
            backend.m_socket.connectToServer(engine.server.serverName());
            await([&] { return backend.m_socket.state()==QLocalSocket::ConnectedState; },"private selection IPC");
            bool hello=false; backend.request("hello",{},{{"token","synthetic-operation-token"}},[&](auto,bool ok){hello=ok;});
            await([&]{return hello;},"selection handshake");
            backend.m_connected=true; backend.m_initialAccountsLoaded=true; backend.m_initialPreferencesLoaded=true;
        }
        ~Harness() { backend.m_shutdown=true; backend.m_connected=false; backend.m_socket.abort(); }
        Held take() { await([&]{return !writes.isEmpty();},"selected mutation delivered"); return writes.takeFirst(); }
        void finish(const Held &held, bool ok=true) {
            const auto params=held.request["params"].toObject();
            if (ok) held.peer->write(reply(held.request["id"].toString(),{{"id",params["id"]},{"revision",4},{"message_ids",params["message_ids"]},{"messages",QJsonArray{}}}));
            else held.peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",held.request["id"]},{"ok",false},{"error",QJsonObject{{"code","stale_revision"},{"message","Conversation changed"}}}}).toJson(QJsonDocument::Compact)+'\n');
        }
    };
    static void mutations() {
        Harness h;
        h.backend.loadConversations(); await([&]{return !h.backend.m_loading;},"account mailbox loads");
        require(h.lastQuery["folder"]=="received","Inbox includes received labeled mail and excludes sent-only conversations");
        const auto snapshots=h.backend.snapshotMailRows({"a","b"});
        auto changed=h.rows[1].toMap(); changed["message_ids"]=QVariantList{"b-original","b-later"}; changed["revision"]=4;
        h.rows[1]=changed; h.backend.publishConversations(h.rows);
        h.backend.applyMailSelection(snapshots,"done");
        auto first=h.take();
        const auto params=first.request["params"].toObject();
        require(first.request["account"]=="personal" && params["read"]==true && params["starred"]==false,"Done preserves shared atomic flag semantics");
        require(params["message_ids"].toArray()==QJsonArray{"a-original"},"exact selected first member");
        require(h.writes.isEmpty(),"bulk writes serialized"); h.finish(first);
        const auto second=h.take();
        require(second.request["params"].toObject()["message_ids"].toArray()==QJsonArray{"b-original"},"later arrival not adopted into bulk action");
        require(second.request["params"].toObject()["expected_revision"]==3,"original revision retained for conflict detection");
        h.finish(second,false); await([&]{return !h.backend.m_mutatingConversation;},"conflict stops batch");
        require(h.backend.mailSelectionStatus().startsWith("1 of 2 updated. Stopped"),"partial failure disclosed");
        require(h.writes.isEmpty(),"no automatic retry after failure");
        h.backend.clearError();
        auto foreign=snapshots; auto f=foreign[0].toMap(); f["selection_account"]="other"; foreign[0]=f;
        h.backend.applyMailSelection(foreign,"delay"); require(!h.backend.m_mutatingConversation && h.writes.isEmpty(),"foreign account refused before write");
        h.backend.clearError();
        h.backend.applyMailSelection(h.backend.snapshotMailRows({"a","c"}),"delay"); first=h.take();
        require(first.request["params"].toObject()["starred"]==true && !first.request["params"].toObject().contains("read"),"Delay never reviews mail");
        h.backend.m_account="other"; h.finish(first); await([&]{return !h.backend.m_mutatingConversation;},"switch stops remaining targets");
        require(h.writes.isEmpty(),"queued selection cannot cross account switch");
        Harness restored;
        restored.backend.m_selected="a"; restored.backend.m_conversation=restored.rows[0].toMap();
        const auto retainedReader=restored.backend.m_conversation;
        restored.backend.applyMailSelection(restored.backend.snapshotMailRows({"a","b"}),"done");
        auto prior=restored.take();
        auto account=restored.backend.m_allAccounts[0].toMap(); account["lifecycle"]=QVariantMap{{"state","active"},{"revision",3}};
        restored.backend.m_allAccounts[0]=account;
        restored.finish(prior);
        await([&]{return !restored.backend.m_mutatingConversation;},"remove-restore stops batch");
        require(restored.writes.isEmpty() && restored.backend.m_conversation==retainedReader,"late receipt cannot replace restored account reader");
    }
    static QQuickItem *find(QQuickItem *item,const QString &name) {
        if(item->objectName()==name) return item;
        for(auto *child:item->childItems()) if(auto *found=find(child,name)) return found;
        return nullptr;
    }
    static void click(QQuickWindow *window,QQuickItem *item,Qt::MouseButton button,Qt::KeyboardModifiers modifiers={}) {
        if (!item || !item->isVisible()) qInfo()<<"hidden target"<<(item ? item->objectName() : "missing");
        require(item && item->isVisible(),"actual selection target visible");
        const auto p=item->mapToScene(item->boundingRect().center());
        QMouseEvent down(QEvent::MouseButtonPress,p,window->mapToGlobal(p.toPoint()),button,button,modifiers);
        QCoreApplication::sendEvent(window,&down);
        QMouseEvent up(QEvent::MouseButtonRelease,p,window->mapToGlobal(p.toPoint()),button,Qt::NoButton,modifiers);
        QCoreApplication::sendEvent(window,&up);
    }
    static void controls() {
        Harness h;
        QTemporaryDir copy; const auto modulePath=copy.filePath("SelectionTest"); require(QDir().mkpath(modulePath),"selection QML module");
        QByteArray module("module SelectionTest\nsingleton Theme 1.0 Theme.qml\n");
        for(const auto &name:QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"},QDir::Files)) {
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name),"unchanged production QML");
            if(name!="Theme.qml") module+=name.chopped(4).toUtf8()+" 1.0 "+name.toUtf8()+'\n';
        }
        QFile registration(modulePath+"/qmldir"); require(registration.open(QIODevice::WriteOnly),"selection registration"); registration.write(module); registration.close();
        qmlRegisterType<ReplyDocument>("SelectionTest",1,0,"ReplyDocument");
        QQmlApplicationEngine qml; bool warnings=false;
        QObject::connect(&qml,&QQmlEngine::warnings,[&](const auto &errors){warnings=true;for(const auto &error:errors) qWarning()<<error.toString();});
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } StartupOverlay { id:splash; anchors.fill:parent; startupReady:true } }",QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!qml.rootObjects().isEmpty(),"actual MailView and splash load");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"selection window exposed");
        auto *splash=find(window->contentItem(),"startupOverlay"); require(splash,"real splash exists");
        QElapsedTimer elapsed; elapsed.start();
        while(elapsed.elapsed()<1450) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        require(!splash->property("dismissed").toBool() && splash->opacity()==1,"fast startup displays complete lettering before fade");
        await([&]{return splash->property("dismissed").toBool();},"splash eventually releases input");
        require(elapsed.elapsed()>=1700,"minimum splash hold respected");
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"actual mail workspace");
        auto summaryRow=h.rows[0].toMap(); summaryRow["summary"]="Review the design by Friday."; summaryRow["snippet"]="Original body preview"; h.rows[0]=summaryRow;
        h.backend.publishConversations(h.rows);
        await([&]{auto *preview=find(window->contentItem(),"mailPreview_a");return preview && preview->property("text")=="Review the design by Friday.";},"summary replaces source preview");
        require(find(window->contentItem(),"summaryPreviewIcon_a")->isVisible(),"summary preview has assistance icon");
        require(!find(window->contentItem(),"threadSummary") && !find(window->contentItem(),"summarizeConversation"),"no redundant thread summary surface");
        summaryRow.remove("summary");h.rows[0]=summaryRow;h.backend.publishConversations(h.rows);
        await([&]{return find(window->contentItem(),"mailPreview_a")->property("text").toString().contains("Original body preview");},"missing summary falls back to source text");
        require(!find(window->contentItem(),"summaryPreviewIcon_a")->isVisible(),"source preview has no assistance icon");
        click(window,find(window->contentItem(),"conversationRow_a"),Qt::RightButton);
        await([&]{return find(window->contentItem(),"contextDone")->isVisible();},"right click opens actual menu");
        require(h.writes.isEmpty() && !h.methods.contains("conversation"),"right click is not deliberate open/read");
        QKeyEvent escape(QEvent::KeyPress,Qt::Key_Escape,Qt::NoModifier); QCoreApplication::sendEvent(window,&escape);
        click(window,find(window->contentItem(),"conversationRow_a"),Qt::LeftButton,Qt::ControlModifier);
        click(window,find(window->contentItem(),"conversationRow_c"),Qt::LeftButton,Qt::ShiftModifier);
        require(mail->property("selectionRows").toList().size()==3,"Ctrl click plus Shift range selects three without opening");
        require(h.writes.isEmpty(),"multi-selection never marks read");
        click(window,find(window->contentItem(),"conversationRow_b"),Qt::RightButton);
        await([&]{return find(window->contentItem(),"contextDone")->isVisible();},"bulk context menu");
        require(!find(window->contentItem(),"contextReply")->isVisible(),"reply unavailable for multiple conversations");
        click(window,find(window->contentItem(),"contextDone"),Qt::LeftButton);
        for(int i=0;i<3;++i) h.finish(h.take());
        await([&]{return !h.backend.m_mutatingConversation;},"native bulk Done completes");
        require(h.backend.mailSelectionStatus().startsWith("3 of 3"),"native selection result visible");
        h.backend.loadCategories();
        await([&]{return !h.backend.categories().isEmpty();},"label catalog available");
        click(window,find(window->contentItem(),"conversationRow_a"),Qt::RightButton);
        click(window,find(window->contentItem(),"contextLabels"),Qt::LeftButton);
        await([&]{auto *item=find(window->contentItem(),"applyLabel_imported");return item && item->isVisible();},"label submenu opens");
        QElapsedTimer layout; layout.start(); while(layout.elapsed()<100) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"applyLabel_imported"),Qt::LeftButton);
        auto label=h.take();
        require(label.request["method"]=="set_category" && label.request["params"].toObject()["category_id"]=="imported" && label.request["params"].toObject()["present"]==true,"submenu applies exact chosen label");
        h.finish(label); await([&]{return !h.backend.m_mutatingConversation;},"label action completes");
        QMetaObject::invokeMethod(mail,"clearSelection");
        mail->setProperty("selectionAnchor","a");
        h.backend.m_selected="a";
        find(window->contentItem(),"conversationRow_a")->forceActiveFocus();
        QKeyEvent rangeKey(QEvent::KeyPress,Qt::Key_Down,Qt::ShiftModifier); QCoreApplication::sendEvent(window,&rangeKey);
        await([&]{return mail->property("selectionRows").toList().size()==2;},"Shift+Down extends the focused list selection");
        QKeyEvent deleteKey(QEvent::KeyPress,Qt::Key_Delete,Qt::NoModifier); QCoreApplication::sendEvent(window,&deleteKey);
        auto deleted=h.take();
        require(deleted.request["method"]=="mail_filing" && deleted.request["params"].toObject()["action"]=="trash" && deleted.request["params"].toObject()["message_ids"].toArray()==QJsonArray{"a-original"},"Delete files the exact first selected message to Trash");
        h.finish(deleted); deleted=h.take();
        require(deleted.request["params"].toObject()["message_ids"].toArray()==QJsonArray{"b-original"},"Keyboard deletion preserves the selected range");
        h.finish(deleted); await([&]{return !h.backend.m_mutatingConversation;},"keyboard deletion finishes");
        await([&]{return !h.backend.m_loading;},"list refresh after label");
        layout.restart(); while(layout.elapsed()<100) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"conversationRow_b"),Qt::RightButton);
        await([&]{return find(window->contentItem(),"trashConversation")->isVisible();},"Trash context visible");
        click(window,find(window->contentItem(),"trashConversation"),Qt::LeftButton);
        auto trash=h.take();
        require(trash.request["method"]=="mail_filing" && trash.request["params"].toObject()["action"]=="trash","menu uses recoverable Trash action");
        h.finish(trash); await([&]{return !h.backend.m_mutatingConversation;},"Trash completes");
        h.backend.setFolder("sent");
        await([&]{return !h.backend.m_loading && h.lastQuery["folder"]=="sent";},"Sent loads");
        require(h.lastQuery["grouped"]==false,"Sent sorts by date instead of review state");
        require(mail->property("dateGroups").toBool(),"Sent has date grouping by default");
        QQuickItem *header=nullptr;
        std::function<QQuickItem*(QQuickItem*)> renderedHeader=[&](QQuickItem *item)->QQuickItem* {
            if(item->objectName().startsWith("mailGroup_") && item->isVisible())return item;
            for(auto *child:item->childItems())if(auto *found=renderedHeader(child))return found;
            return nullptr;
        };
        await([&]{header=renderedHeader(window->contentItem());return header;},"date header painted after layout");
        click(window,header,Qt::LeftButton);
        require(!find(window->contentItem(),"conversationRow_a")->isVisible(),"date group collapses");
        h.backend.m_preferences["mail_date_grouping"]="plain"; emit h.backend.preferencesChanged();
        await([&]{return find(window->contentItem(),"conversationRow_a")->isVisible();},"plain list restores rows");
        require(!mail->property("dateGroups").toBool(),"shared plain-list preference applied");
        h.backend.m_folder="inbox"; h.backend.m_selected="a"; emit h.backend.queryChanged();
        h.backend.m_conversation={{"id","a"},{"subject","Source inspection"},{"revision",3},{"message_ids",QVariantList{"a-original"}},{"messages",QVariantList{QVariantMap{{"id","a-original"},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text","Retained plain body"},{"source_available",true},{"source_version",QString(64, char(97))},{"sent",false}}}}};
        emit h.backend.conversationChanged();
        await([&]{auto item=find(window->contentItem(),"messageBody_a-original");return item && item->property("text").toString().contains("Retained HTML");},"received HTML loads for the expanded message");
        require(h.writes.isEmpty(),"source inspection queues no mutations");
        auto body=find(window->contentItem(),"messageBody_a-original")->property("text").toString();
        require(!body.contains("<img") && !body.contains("images.example.test"),"received body contains no image resources");
        QMetaObject::invokeMethod(mail,"scrollToMember",Q_ARG(QVariant,QString("a-original")));
        layout.restart(); while(layout.elapsed()<120)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"messageMore_a-original"),Qt::LeftButton);
        await([&]{auto item=find(window->contentItem(),"contextFormat");return item && item->isVisible();},"per-message format menu");
        layout.restart(); while(layout.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"contextFormat"),Qt::LeftButton);
        await([&]{auto item=find(window->contentItem(),"messageFormatPlain");return item && item->isVisible();},"format submenu");
        layout.restart(); while(layout.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"messageFormatPlain"),Qt::LeftButton);
        await([&]{return find(window->contentItem(),"messageBody_a-original")->property("text").toString().contains("Retained plain body");},"plain format uses retained extracted body");
        QMetaObject::invokeMethod(mail,"scrollToMember",Q_ARG(QVariant,QString("a-original")));
        layout.restart(); while(layout.elapsed()<120)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"messageMore_a-original"),Qt::LeftButton);
        await([&]{auto item=find(window->contentItem(),"contextMessageSource");return item && item->isVisible();},"per-message source option");
        layout.restart(); while(layout.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"contextMessageSource"),Qt::LeftButton);
        await([&]{auto item=find(window->contentItem(),"messageSourceText_a-original");return item && item->isVisible();},"source dialog opens");
        require(find(window->contentItem(),"messageSourceText_a-original")->property("text").toString().startsWith("From: Sender <sender@example.test>"),"source displays retained headers as plain text");
        require(h.writes.isEmpty(),"format and source remain read only");
        const auto sourceCapture=qEnvironmentVariable("FM_SOURCE_CAPTURE");
        if(!sourceCapture.isEmpty()) { const auto sourceFont=find(window->contentItem(),"messageSourceText_a-original")->property("font").value<QFont>(); qInfo()<<"Source font"<<sourceFont<<QFontInfo(sourceFont).family(); layout.restart();while(layout.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);require(window->grabWindow().save(sourceCapture),"source dialog frame"); }
        QKeyEvent dismissSource(QEvent::KeyPress,Qt::Key_Escape,Qt::NoModifier);QCoreApplication::sendEvent(window,&dismissSource);
        await([&]{auto item=find(window->contentItem(),"messageSourceText_a-original");return !item || !item->isVisible();},"Escape closes source dialog");
        require(!warnings,"no QML errors in production selection/splash controls");
        const auto capture=qEnvironmentVariable("FM_SELECTION_CAPTURE");
        if(!capture.isEmpty()) {
            h.backend.m_folder="inbox"; emit h.backend.queryChanged();
            auto preview=h.rows[0].toMap();preview["summary"]="You granted Example Mail access to your account data. Review access if you did not authorize it.";preview["subject"]="Security alert";h.rows[0]=preview;h.backend.publishConversations(h.rows);
            h.backend.m_selected="a";
            h.backend.m_conversation={{"id","a"},{"subject","Security alert"},{"revision",3},{"messages",QVariantList{QVariantMap{{"id","a-original"},{"subject","Security alert"},{"sender_name","Account security"},{"sender_email","accounts@example.test"},{"recipient","test@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text","You allowed Example Mail access to some of your account data. If you did not authorize this, review your account access."},{"kind","automated"},{"sent",false},{"revision",1},{"categories",QVariantList{}}}}}};
            emit h.backend.conversationChanged();
            await([&]{auto *item=find(window->contentItem(),"messageSummary_a-original");return item && item->isVisible();},"per-message summary painted");
            QElapsedTimer paint;paint.start();while(paint.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(window->grabWindow().save(capture),"native mail summary/list capture");
        }
    }
public:
    static void run(){mutations();controls();qInfo()<<"Inbox scope, immutable bulk targets, partial failure/account fences, actual context/range selection and splash timing pass.";}
};
int main(int argc,char **argv){QGuiApplication app(argc,argv);QQuickStyle::setStyle("Basic");for(const auto *font:{"Geist-Regular.ttf","Geist-Medium.ttf","Geist-SemiBold.ttf"}) require(QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR)+"/fonts/"+font)>=0,"bundled font loads");QGuiApplication::setFont(QFont("Geist",10));MailSelectionTest::run();return 0;}
