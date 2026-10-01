// Mail workspace UI test on a real MailBackend over the scripted engine: selection
// and read-on-open, review/Done/Delay, keyboard navigation, search highlights,
// sender cards, attachments, Spam, remote search and the Accounts settings rows.
#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include "WindowPlacement.h"
#include <QGuiApplication>
#include <QFontInfo>
#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlComponent>
#include <QLocale>
#include <QSGRendererInterface>
#include <QQmlContext>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QQuickItem>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QImage>
#include <QCryptographicHash>
#include <QBuffer>
#include <QTextBlock>
#include <QTextDocument>
#include <QTextLayout>
#include <QClipboard>
#include <QColor>
#include <algorithm>
#include <QTest>
#include <QtMath>
using namespace GoogleOperationFixture;

// Conversation rows the list shows, not its section headers and empty lines.
static int shownRows(MailBackend &backend) {
    int n=0; auto *model=backend.conversationModel();
    for (int i=0;i<model->rowCount();++i) if (model->data(model->index(i,0),ConversationListModel::ItemTypeRole).toString()=="row") ++n;
    return n;
}
class MailSelectionTest {
    struct Held { QPointer<QLocalSocket> peer; QJsonObject request; };
    struct Harness {
        Engine engine;
        MailBackend backend{engine.profile,"unused"};
        QList<Held> writes;
        QStringList methods;
        QJsonObject lastQuery;
        QHash<QString,QJsonObject> attachments;
        QList<QJsonObject> remoteSearches;
        QVariantList rows;
        QJsonObject summary; // Core's section counts, merged into list replies
        // Like Core, leave collapsed Done months out of list replies; while
        // holdLists is set, replies wait for releaseLists(), as behind a busy sync.
        bool filterCollapsed = false, holdLists = false;
        // Pages list replies like Core when set: this many rows per offset.
        int pageSize = 0;
        QJsonValue unsubscribed = QJsonValue::Null; // Core keeps a list's unsubscribe
        // Reply drafts by draft ID; a conversation's own reply uses its ID, as in Core.
        QHash<QString,QJsonObject> drafts;
        QList<Held> heldLists;
        // Saved views; one naming missingMember is refused as Core refuses a removed message.
        QList<QJsonObject> viewSaves;
        QString missingMember;
        // Core's Done group for a row: This Week from the requested weekday,
        // the rest of this month, then its month. Row times here are midday UTC.
        static QString doneGroup(const QVariantMap &row, const QJsonObject &params) {
            static const QStringList days{"monday","tuesday","wednesday","thursday","friday","saturday","sunday"};
            const auto today = QDate::fromString(params["today"].toString(), Qt::ISODate);
            const auto day = QDate::fromString(row.value("latest_at").toString().left(10), Qt::ISODate);
            const auto start = days.indexOf(params["done_week_start"].toString());
            if (today.isValid() && start >= 0 && day >= today.addDays(-((today.dayOfWeek() - 1 - start + 7) % 7))) return "week";
            if (today.isValid() && day.year() == today.year() && day.month() == today.month()) return "month";
            return row.value("latest_at").toString().left(7);
        }
        QJsonObject listReply(const QJsonObject &params) const {
            QJsonArray shown;
            const auto months = params["collapsed_months"].toArray(), open = params["open_months"].toArray();
            for (const auto &value : rows) {
                const auto row = value.toMap();
                const auto month = doneGroup(row, params);
                const bool older = month != "week" && month != "month";
                if (filterCollapsed && row.value("group") == "done"
                    && (months.contains(month) || (params.contains("open_months") && older && !open.contains(month)))) continue;
                shown.append(QJsonObject::fromVariantMap(row));
            }
            QJsonValue next = QJsonValue::Null;
            int total = 3;
            if (pageSize > 0) {
                const int offset = params["offset"].toInt(), all = int(shown.size());
                QJsonArray page; for (int i = offset; i < std::min(all, offset + pageSize); ++i) page.append(shown[i]);
                if (offset + pageSize < all) next = offset + pageSize;
                shown = page; total = all;
            }
            QJsonObject result{{"conversations",shown},{"revision",3},{"filtered_conversation_count",total},{"folder_conversation_count",total},{"next_offset",next}};
            for (auto it=summary.begin(); it!=summary.end(); ++it) result[it.key()]=it.value();
            return result;
        }
        void releaseLists() {
            for (const auto &held : std::as_const(heldLists)) if (held.peer) held.peer->write(reply(held.request["id"].toString(),listReply(held.request["params"].toObject())));
            heldLists.clear();
        }
        Harness() {
            backend.m_account = "personal";
            backend.m_syntheticProfile = false;
            backend.m_preferences = {{"notifications","off"},{"theme","graphite"},{"text_size","default"}};
            // A fixed date, with September an older month the user opened.
            backend.m_mailToday = QDate(2026,10,14);
            backend.m_openSections = {"done:2026-09"};
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
                const auto draftId = request["params"].toObject()["draft_id"].toString(request["params"].toObject()["conversation_id"].toString());
                if (method == "draft_discard" && drafts.contains(draftId)) {
                    auto draft = drafts[draftId]; draft["discarded"] = true; draft["body"] = "";
                    draft["revision"] = request["params"].toObject()["expected_revision"].toInteger()+1; drafts[draftId] = draft;
                    peer->write(reply(request["id"].toString(),draft));
                }
                else if (method == "flag" || method == "set_category" || method == "mail_filing" || method == "draft_discard" || method == "mail_undo") writes.append({peer,request});
                else {
                    QJsonObject result;
                    if (method == "accounts") result["accounts"] = QJsonArray::fromVariantList(backend.m_allAccounts);
                    else if (method == "conversations") {
                        lastQuery = request["params"].toObject();
                        if (holdLists) { heldLists.append({peer,request}); return true; }
                        result = listReply(lastQuery);
                    }
                    else if (method == "assist_results") result={{"items",QJsonArray{QJsonObject{{"task","summary"},{"target","a-original"},{"stale",false},{"partial",false},{"source_ids",QJsonArray{"a-original"}},{"schema",2},{"output",QJsonObject{{"summary","You granted Example Mail access to your account data. Review access if you did not authorize it."},{"source_ids",QJsonArray{"a-original"}}}},{"model",QJsonObject{{"name","gemma4:e4b"}}}}}}};
                    else if (method == "assist_settings_get") result={{"revision",0},{"settings",QJsonObject{{"accounts",QJsonObject{}},{"model",QJsonObject{{"name","example.registry/team/a-very-long-qualified-model-name:large"},{"digest","synthetic"}}}}}};
                    else if (method == "message_source") {
                        const auto params = request["params"].toObject();
                        result = {{"account","personal"},{"id",params["id"]},{"source_version",params["source_version"]},{"raw","From: Sender <sender@example.test>\r\nTo: Me <me@example.test>\r\n\r\nRetained body"},{"body_html","<p><b>Retained HTML</b><img src=\"https://images.example.test/pixel\" alt=\"Logo\"></p>"},{"metadata",QJsonArray{QJsonObject{{"label","Cc"},{"people",QJsonArray{QJsonObject{{"name","Team"},{"email","team@example.test"}}}}}}}};
                    }
                    else if (method == "attachment") result = attachments.value(request["params"].toObject()["id"].toString());
                    else if (method == "google_spam_list") result = {{"messages",QJsonArray{QJsonObject{{"id","s1"},{"thread_id","t9"},{"sender_name","Prize Desk"},{"sender_email","win@example.test"},{"subject","You won"},{"snippet","Claim now"},{"internal_date_ms",1789210800123LL},{"unread",true}},
                        QJsonObject{{"id","s0"},{"thread_id","t9"},{"sender_name","Prize Desk"},{"sender_email","win@example.test"},{"subject","You won"},{"snippet","First"},{"internal_date_ms",1789210700123LL},{"unread",false}}}},{"next_page_token",QJsonValue()}};
                    else if (method == "google_bin_counts") result = {{"folder",request["params"].toObject()["folder"]},{"messages",2},{"conversations",1}};
                    else if (method == "google_not_spam") result = {{"thread_id",request["params"].toObject()["thread_id"]},{"moved",2}};
                    else if (method == "google_delete_forever") { peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",request["id"]},{"ok",false},{"error",QJsonObject{{"code","reconnect_required"},{"message","Reconnect this account to allow deleting mail for good"}}}}).toJson(QJsonDocument::Compact)+'\n'); return true; }
                    else if (method == "google_remote_search") { remoteSearches.append(request["params"].toObject()); result = {{"conversations",QJsonArray{QJsonObject{{"id","remote:t1"},{"remote",true},{"remote_thread_id","t1"},{"subject","Old invoice"},{"sender_name","Billing"},{"sender_email","billing@example.test"},{"snippet","Your invoice"},{"latest_at","2020-01-05T09:00:00Z"},{"message_count",2},{"group","done"},{"kind","unknown"},{"unread",false},{"starred",false},{"revision",0},{"message_ids",QJsonArray{}},{"categories",QJsonArray{}}}}}}; }
                    else if (method == "google_remote_thread") result = {{"id","remote:"+request["params"].toObject()["thread_id"].toString()},{"remote",true},{"subject","Old invoice"},{"revision",0},{"message_ids",QJsonArray{}},{"categories",QJsonArray{}},{"messages",QJsonArray{
                        QJsonObject{{"id","remote-m1"},{"sender_name","Billing"},{"sender_email","billing@example.test"},{"sent_at","2020-01-04T09:00:00Z"},{"text","First"},{"remote",true},{"source_available",false},{"source_version",""},{"attachments",QJsonArray{}},{"sent",false}},
                        QJsonObject{{"id","remote-m2"},{"sender_name","Billing"},{"sender_email","billing@example.test"},{"sent_at","2020-01-05T09:00:00Z"},{"text","Invoice attached"},{"body_html","<p><b>Invoice</b> attached</p>"},{"remote",true},{"source_available",false},{"source_version",""},{"sent",false},
                            {"attachments",QJsonArray{QJsonObject{{"id","remote-att:m2:0"},{"name","invoice.pdf"},{"mime_type","application/pdf"},{"size",4},{"state","remote"}}}}}}}};
                    else if (method == "message_unsubscribe") { result = {{"method","one_click"},{"at","2026-09-25T10:00:00Z"}}; unsubscribed = QJsonObject{{"method","one_click"},{"at","2026-09-25T10:00:00Z"}}; }
                    else if (method == "google_unsubscribe_mail") { result = {{"method","mailto"},{"address","leave@example.test"},{"at","2026-09-25T10:00:00Z"}}; unsubscribed = QJsonObject{{"method","mailto"},{"at","2026-09-25T10:00:00Z"}}; }
                    else if (method == "categories") result["categories"] = QJsonArray{QJsonObject{{"id","imported"},{"name","Imported folder"},{"conversation_count",3}}};
                    else if (method == "view_save") {
                        const auto params = request["params"].toObject(); viewSaves.append(params);
                        if (!missingMember.isEmpty() && params["view"].toObject()["selectedMember"] == missingMember) {
                            peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",request["id"]},{"ok",false},{"error",QJsonObject{{"code","not_found"},
                                {"message","Selected member is outside this account"},{"details",QJsonObject{{"view_key","selectedMember"}}}}}}).toJson(QJsonDocument::Compact)+'\n');
                            return true;
                        }
                        result["revision"] = 1;
                    }
                    else if (method == "draft_get" && drafts.contains(draftId)) result = drafts[draftId];
                    else if (method == "draft_get") result = {{"conversation_id",request["params"].toObject()["conversation_id"]},{"account","personal"},{"revision",0}};
                    else if (method == "draft_save" && drafts.contains(draftId)) {
                        const auto params = request["params"].toObject(); auto draft = drafts[draftId];
                        draft["body"] = params["body"]; draft["discarded"] = false; draft["revision"] = params["expected_revision"].toInteger()+1; drafts[draftId] = draft; result = draft;
                    }
                    else if (method == "draft_create") {
                        const auto params = request["params"].toObject(); const auto id = QString("reply-%1").arg(drafts.size());
                        const auto body = params["mode"]=="forward" ? QString("\n\n---------- Forwarded message ----------\n> Retained plain body") : QString();
                        result = {{"account","personal"},{"draft_id",id},{"conversation_id",params["conversation_id"]},{"mode",params["mode"]},{"source_message_id",params["source_message_id"]},
                            {"revision",1},{"discarded",false},{"body",body},{"subject",params["mode"]=="new" ? "" : "Re: Labeled message a"},
                            {"to_recipients",params["mode"]=="new" ? QJsonArray{} : QJsonArray{QJsonObject{{"email","sender@example.test"},{"name","Sender"}}}},
                            {"cc_recipients",QJsonArray{}},{"bcc_recipients",QJsonArray{}},{"attachment_ids",QJsonArray{}},{"attachments",QJsonArray{}}};
                        drafts[id] = result;
                    }
                    else if (method == "conversation") {
                        const auto id = request["params"].toObject()["id"].toString();
                        // A thread read reports the row's current members and revision.
                        QJsonArray ids{id+"-original"}; qint64 revision=3;
                        for (const auto &row : rows) if (row.toMap().value("id").toString()==id) {
                            ids=QJsonArray::fromVariantList(row.toMap().value("message_ids").toList()); revision=row.toMap().value("revision").toLongLong();
                        }
                        result = {{"id",id},{"revision",revision},{"message_ids",ids},{"messages",QJsonArray{QJsonObject{{"id",id+"-original"},{"unread",true},{"starred",false},{"sender_name","Sender"},{"sent_at","2026-09-14T10:00:00Z"},{"text","Retained plain body"},{"source_available",true},{"source_version",QString(64,char(97))},{"sent",false}}}},
                            {"mailing_list",QJsonObject{{"sender_email","sender@example.test"},{"month",2},{"year",9},{"unsubscribed",unsubscribed}}}};
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
        // Like Core's conversation projection, every receipt carries the conversation's labels.
        QJsonArray labels(const QString &id) const {
            for (const auto &row : rows) if (row.toMap().value("id").toString()==id) return QJsonArray::fromVariantList(row.toMap().value("categories").toList());
            return {};
        }
        void finish(const Held &held, bool ok=true) {
            const auto params=held.request["params"].toObject();
            if (ok) held.peer->write(reply(held.request["id"].toString(),{{"id",params["id"]},{"revision",4},{"message_ids",params["message_ids"]},{"categories",labels(params["id"].toString())},{"messages",QJsonArray{}}}));
            else held.peer->write(QJsonDocument(QJsonObject{{"version",1},{"id",held.request["id"]},{"ok",false},{"error",QJsonObject{{"code","stale_revision"},{"message","Conversation changed"}}}}).toJson(QJsonDocument::Compact)+'\n');
        }
    };
    static void readerMemorySharing() {
        Harness h;
        h.backend.m_selected="a";
        const auto version=QString(64,'a'), key=QString("personal\na-original\n")+version;
        const QVariantMap source{{"account","personal"},{"id","a-original"},{"source_version",version},
            {"body_html",QString("<p>Generated retained source.</p>").repeated(6000)},
            {"metadata",QVariantList{QVariantMap{{"label","From"},{"value","sender@example.test"}}}}};
        h.backend.m_sourceCache.insert(key,new MailBackend::ReaderSnapshot{source,1},400000);
        const QVariantMap member{{"id","a-original"},{"source_available",true},{"source_version",version},
            {"text","Generated body"},{"reader_sections",QVariantList{}},{"reader_plain_sections",QVariantList{}}};
        const QVariantMap data{{"id","a"},{"revision",3},{"messages",QVariantList{member}}};
        const auto requests=h.methods.count("message_source");
        for(int round=0;round<12;++round) {
            QVariantMap prepared; bool ready=false;
            h.backend.prepareReader("personal","a",data,h.backend.m_generation,1,false,[&](auto value){prepared=value;ready=true;});
            await([&]{return ready;},"cached reader prepares");
            const auto kept=prepared.value("messages").toList().last().toMap().value("reader_source").toMap();
            require(kept==source && kept.isSharedWith(source),"reader retains the exact cached source without copying its map");
            require(kept.value("body_html").toString().constData()==source.value("body_html").toString().constData(),"large source text shares its allocation");
            require(h.backend.m_sourceCache.object(key)->data.isSharedWith(source),"preparing a cache hit preserves the cached source");
            h.backend.cacheConversation("personal","a",prepared,1);
            const auto cached=h.backend.m_readerCache.object(qMakePair(QString("personal"),QString("a")));
            h.backend.cacheConversation("personal","a",prepared,1);
            require(h.backend.m_readerCache.object(qMakePair(QString("personal"),QString("a")))==cached,"an unchanged reader is kept in place");
        }
        require(h.methods.count("message_source")==requests,"cache reuse adds no source request");
        auto changed=data; changed["subject"]="Fresh metadata at the same revision";
        h.backend.cacheConversation("personal","a",changed,1);
        require(h.backend.m_readerCache.object(qMakePair(QString("personal"),QString("a")))->data==changed,"same-revision metadata changes still refresh the reader");
        auto older=changed;older["revision"]=2;
        h.backend.cacheConversation("personal","a",older,1);
        require(h.backend.m_readerCache.object(qMakePair(QString("personal"),QString("a")))->data==changed,"older replies cannot overwrite cached data");
        qInfo()<<"Repeated reader preparations share"<<source.value("body_html").toString().size()*2<<"source-text bytes and preserve cached replies.";
    }
    static void scopedRefreshes() {
        Harness h;
        auto &backend = h.backend;
        backend.m_selected = "a";
        backend.m_calendarRevision = 0; backend.m_mailRevision = 0; backend.m_revision = 0;
        QJsonObject generations{{"epoch","synthetic-store"},{"accounts",0},{"categories",0},{"reader",0},{"drafts",0},{"captures",0}};
        backend.m_uiGenerations = generations;
        QJsonObject status{{"revision",0},{"calendar_generation",0},{"mail_generation",0},{"preferences_revision",0},{"ui_generations",generations}};
        const auto settle = [&] {
            await([&]{ return backend.m_pending.isEmpty() && backend.m_readerLoadingGeneration < 0; }, "scoped refresh finishes");
            backend.m_draftChipsTimer.stop();
        };
        const auto poll = [&](const QString &area) {
            settle(); h.methods.clear();
            if (!area.isEmpty()) generations[area] = generations[area].toInteger() + 1;
            status["revision"] = status["revision"].toInteger() + 1;
            status["ui_generations"] = generations;
            backend.refreshFromStatus(status); settle();
        };
        poll({});
        require(h.methods.isEmpty(), "global revision alone does not reload independent projections");
        poll("accounts");
        require(h.methods == QStringList{"accounts"}, "sync/account metadata only refreshes accounts");
        poll("categories");
        require(h.methods == QStringList{"categories"}, "category generation only requests category projection");
        poll("captures");
        require(h.methods == QStringList{"capture_history"}, "capture generation only requests history");
        poll("drafts");
        require(h.methods.contains("draft_get") && h.methods.contains("google_sent")
            && !h.methods.contains("conversation") && !h.methods.contains("assist_results") && !h.methods.contains("capture_history"),
            "draft updates retain send status without preparing the received reader");
        backend.m_draftDirty = true; backend.m_draft["body"] = "Unsaved human text";
        poll("drafts");
        require(backend.m_draft.value("body") == "Unsaved human text" && backend.m_draftDirty,
            "external draft polling cannot overwrite unsaved typing");
        backend.m_draftDirty = false;
        poll("reader");
        require(h.methods.contains("conversation") && h.methods.contains("assist_results")
            && !h.methods.contains("draft_get") && !h.methods.contains("capture_history") && h.writes.isEmpty(),
            "changed mail refreshes the reader without marking it read or reloading unchanged drafts");
        require(backend.m_conversation.value("id") == "a", "changed reader is published");

        backend.m_readerLoadingGeneration = backend.m_generation;
        generations["reader"] = generations["reader"].toInteger() + 1;
        status["ui_generations"] = generations; h.methods.clear();
        backend.refreshFromStatus(status);
        require(!backend.m_uiGenerations.contains("reader") && !h.methods.contains("conversation"),
            "a busy reader keeps its invalidation instead of losing a change");
        backend.m_readerLoadingGeneration = -1; poll({});
        require(h.methods.contains("conversation"), "a later unchanged poll refreshes the deferred reader");

        backend.m_queryActivity.start(); status["mail_generation"] = 1;
        poll({});
        require(backend.m_listRefreshPending && !h.methods.contains("conversations"), "typing defers a changed list");
        backend.m_queryActivity.invalidate(); poll({});
        require(!backend.m_listRefreshPending && h.methods.contains("conversations"), "unchanged later status delivers the deferred list");
        backend.m_browsingOlder = true; status["mail_generation"] = 2; poll({});
        require(backend.m_listRefreshPending && !h.methods.contains("conversations"), "older pages retain deferred invalidation");
        backend.m_browsingOlder = false; poll({});
        require(!backend.m_listRefreshPending && h.methods.contains("conversations"), "returning from older pages delivers invalidation");
        generations["epoch"] = "reopened-store"; poll({});
        for (const auto *method : {"accounts","categories","conversation","draft_get","capture_history"})
            require(h.methods.contains(method), "a new store epoch refreshes every projection");
        h.methods.clear(); status.remove("ui_generations"); status["revision"] = status["revision"].toInteger() + 1;
        backend.refreshFromStatus(status); settle();
        for (const auto *method : {"accounts","categories","conversation","draft_get","capture_history"})
            require(h.methods.contains(method), "legacy status retains conservative full refresh");
        require(h.writes.isEmpty(), "background refresh never marks mail read or sends it");
        qInfo() << "Scoped refresh: account-only poll requests 1 projection; unrelated global revision requests 0. Reader, draft, delivery, deferred list and reconnect checks pass.";
    }
    // Copies the production QML into a private SelectionTest module.
    static void copyModule(const QTemporaryDir &copy) {
        const auto modulePath=copy.filePath("SelectionTest"); require(QDir().mkpath(modulePath),"private QML module");
        QByteArray module("module SelectionTest\nsingleton Theme 1.0 Theme.qml\n");
        for(const auto &name:QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"},QDir::Files)) {
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name),"unchanged production QML");
            if(name=="ShortcutRegistry.qml") module+="singleton ";
            if(name!="Theme.qml") module+=name.chopped(4).toUtf8()+" 1.0 "+name.toUtf8()+'\n';
        }
        QFile registration(modulePath+"/qmldir"); require(registration.open(QIODevice::WriteOnly),"module registration"); registration.write(module); registration.close();
        qmlRegisterType<ReplyDocument>("SelectionTest",1,0,"ReplyDocument");
    }
    static void mutations() {
        Harness h;
        h.backend.loadConversations(); await([&]{return !h.backend.m_loading;},"account mailbox loads");
        require(h.lastQuery["folder"]=="inbox","Inbox uses real Inbox membership so Archive removes mail");
        const auto snapshots=h.backend.snapshotMailRows({"a","b"});
        auto changed=h.rows[1].toMap(); changed["message_ids"]=QVariantList{"b-original","b-later"}; changed["revision"]=4;
        h.rows[1]=changed; h.backend.publishConversations(h.rows); emit h.backend.conversationsChanged();
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
        // Unseen work (a summary, a label rule) moved only the revision: the
        // same members are retried once at the current revision, without an error.
        auto moved=h.rows[0].toMap(); moved["revision"]=5; h.rows[0]=moved;
        const auto listed=h.backend.snapshotMailRows({"a"});
        h.backend.applyMailSelection(listed,"read");
        auto stale=h.take();
        require(stale.request["params"].toObject()["expected_revision"]==listed[0].toMap()["revision"].toInt() && listed[0].toMap()["revision"]!=5,
            "selection uses the revision it was shown at");
        h.finish(stale,false);
        auto retried=h.take();
        require(retried.request["params"].toObject()["expected_revision"]==5 && retried.request["params"].toObject()["message_ids"].toArray()==QJsonArray{"a-original"}
            && retried.request["params"].toObject()["request_id"]!=stale.request["params"].toObject()["request_id"],"unchanged members retry at the current revision");
        h.finish(retried); await([&]{return !h.backend.m_mutatingConversation;},"retried read finishes");
        require(h.backend.mailSelectionStatus().isEmpty() && h.backend.error().isEmpty() && h.writes.isEmpty(),"a retried read reports no conflict");
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
        Harness filing;
        filing.backend.m_selected="a"; filing.backend.m_pinnedRow=filing.rows[0].toMap(); filing.backend.m_pinnedIndex=0;
        filing.backend.applyMailSelection(filing.backend.snapshotMailRows({"a"}),"archive");
        const auto archive=filing.take();
        require(archive.request["method"]=="mail_filing" && archive.request["params"].toObject()["action"]=="archive","Archive uses shared filing command");
        filing.rows.removeFirst(); filing.finish(archive);
        await([&]{return !filing.backend.m_mutatingConversation && !filing.backend.m_loading;},"filing refreshes Inbox");
        require(filing.backend.m_pinnedRow.isEmpty() && filing.backend.conversations().size()==2,"archived selected row cannot remain pinned in Inbox");
        Harness reading;
        // Opened and read: the refreshed Inbox returns it as Done, but it stays in
        // To review at its place, already read, while it is open.
        reading.backend.m_folder="inbox"; reading.backend.m_selected="a";
        reading.backend.m_pinnedRow=reading.rows[0].toMap(); reading.backend.m_pinnedIndex=0;
        auto read=reading.rows[0].toMap(); read["group"]="done"; read["unread"]=false; reading.rows.removeFirst(); reading.rows.append(read);
        reading.backend.loadConversations(false,true);
        await([&]{return !reading.backend.m_loading && reading.backend.conversations().size()==3 && reading.backend.conversations().first().toMap()["id"]=="a";},"read open conversation stays first");
        {
            const auto first=reading.backend.conversations().first().toMap();
            require(first["pinned"].toBool() && first["section"]=="review" && !first["unread"].toBool(),"a read open conversation stays in To review, marked read");
        }
        // Opening the next one moves it to Done.
        reading.backend.selectConversation("b");
        await([&]{return !reading.backend.m_loading && reading.backend.conversations().size()==3 && reading.backend.conversations().last().toMap()["id"]=="a";},"leaving it moves the read conversation to Done");
        {
            QStringList sections; for (const auto &row:reading.backend.conversations()) sections.append(row.toMap()["section"].toString());
            require(!reading.backend.conversations().last().toMap()["pinned"].toBool() && sections.count("review")==2 && sections.indexOf("review")==0 && sections.lastIndexOf("review")==1,
                "a read conversation leaves To review once another is opened and sections are not split");
        }
        // Still open but no longer in this view's results: it stays where it was.
        reading.rows.removeLast(); reading.backend.m_selected="a"; reading.backend.m_pinnedRow=read; reading.backend.m_pinnedIndex=0;
        reading.backend.loadConversations(false,true);
        await([&]{return !reading.backend.m_loading && reading.backend.conversations().size()==3;},"filtered-out open conversation is kept");
        require(reading.backend.conversations().last().toMap()["id"]=="a" && reading.backend.conversations().last().toMap()["pinned"].toBool(),"an open conversation the view no longer returns stays in place, inside its section");
    }
    // To review keeps the open conversation after it is read, inside its own
    // section and counted there, until another is opened; Delay moves it at once
    // and Down goes on from where it was.
    static void reviewTransitions() {
        Harness h;
        const auto make=[](const QString &id,const QString &group,bool unread,bool starred,const QString &at){
            return QVariant(QVariantMap{{"id",id},{"subject","Review "+id},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"latest_at",at},
                {"message_ids",QVariantList{id+"-original"}},{"revision",3},{"message_count",1},{"categories",QVariantList{}},
                {"group",group},{"kind","people"},{"unread",unread},{"starred",starred}}); };
        const auto set=[&](const QString &id,const QString &group,bool unread,bool starred){
            for (auto &row:h.rows) if (row.toMap()["id"]==id) { auto m=row.toMap(); m["group"]=group; m["unread"]=unread; m["starred"]=starred; row=m; } };
        // Core's order: sections, then newest first.
        const auto order=[&](const QStringList &ids){ QVariantList sorted; for (const auto &id:ids) for (const auto &row:h.rows) if (row.toMap()["id"]==id) sorted.append(row); h.rows=sorted; };
        const auto counts=[&](int review,int held,int done){ h.summary={{"filtered_group_counts",QJsonObject{{"review",review},{"held",held}}},{"done_months",QJsonArray{QJsonObject{{"month","2026-09"},{"count",done}}}}}; };
        h.rows={make("r1","review",true,false,"2026-09-14T12:00:00Z"),make("r2","review",true,false,"2026-09-14T11:00:00Z"),make("h1","held",false,true,"2026-09-14T09:00:00Z"),
            make("r3","review",true,false,"2026-09-14T08:00:00Z"),make("d1","done",false,false,"2026-09-10T10:00:00Z")};
        order({"r1","r2","r3","h1","d1"}); counts(3,1,1);
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        QObject::connect(&qml,&QQmlEngine::warnings,[&](const auto &errors){for(const auto &error:errors) qWarning()<<error.toString();});
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Review.qml")));
        require(!qml.rootObjects().isEmpty(),"review MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"review window exposed");
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"review mail workspace");
        h.backend.m_folder="inbox"; h.backend.m_selected.clear(); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==5;},"review Inbox loads");
        const auto ids=[&]{ QStringList out; for (const auto &row:h.backend.conversations()) out.append(row.toMap()["id"].toString()); return out; };
        const auto sections=[&]{ QStringList out; for (const auto &row:h.backend.conversations()) out.append(row.toMap()["section"].toString()); return out; };
        const auto unsplit=[&]{ const auto s=sections(); for (int i=1;i<s.size();++i) if (s[i]!=s[i-1] && s.mid(0,i).contains(s[i])) return false; return true; };
        const auto count=[&](const QString &key){ QVariant value; QMetaObject::invokeMethod(mail,"sectionCount",Q_RETURN_ARG(QVariant,value),Q_ARG(QVariant,key)); return value.toInt(); };
        const auto shownCount=[&](const QString &name){ int n=0; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(item->objectName()==name && item->isVisible() && item->height()>0) ++n; for(auto *c:item->childItems()) walk(c); }; walk(window->contentItem()); return n; };
        const auto settle=[&]{ QElapsedTimer t; t.start(); while(t.elapsed()<150) QCoreApplication::processEvents(QEventLoop::AllEvents,10); };
        auto *list=find(window->contentItem(),"conversationList"); require(list,"review list");
        const auto press=[&](int code){ list->forceActiveFocus(); QKeyEvent key(QEvent::KeyPress,code,Qt::NoModifier); QCoreApplication::sendEvent(window,&key); QCoreApplication::processEvents(); };
        settle();
        // Open the first: read at once, kept first in To review and counted there.
        click(window,find(window->contentItem(),"conversationRow_r1"),Qt::LeftButton);
        auto read=h.take(); require(read.request["params"].toObject()["read"]==true,"opening To review reads it");
        set("r1","done",false,false); order({"r2","r3","h1","r1","d1"}); counts(2,1,2); h.finish(read);
        await([&]{return !h.backend.m_mutatingConversation && !h.backend.m_loading && ids().first()=="r1" && h.backend.conversations().first().toMap()["pinned"].toBool();},"read first stays in place");
        settle();
        require(ids()==QStringList{"r1","r2","r3","h1","d1"} && unsplit(),"the kept row stays first in To review");
        require(count("review")==3 && count("done:2026-09")==1,"To review counts the kept row; Done does not count it twice");
        // Down: the next one opens and is read; the first moves to Done, the second is kept.
        press(Qt::Key_Down);
        read=h.take(); require(read.request["params"].toObject()["id"]=="r2","Down opens the next To review item");
        // The first folds away where it is before it moves.
        require(ids().first()=="r1" && h.backend.conversations().first().toMap()["leaving"].toBool(),"the kept row collapses in place when the next opens");
        set("r2","done",false,false); order({"r3","h1","r1","r2","d1"}); counts(1,1,3); h.finish(read);
        await([&]{return !h.backend.m_mutatingConversation && !h.backend.m_loading && ids().first()=="r2";},"second kept after Down");
        settle();
        require(std::none_of(h.backend.conversations().cbegin(),h.backend.conversations().cend(),[](const QVariant &row){return row.toMap()["leaving"].toBool();}),"no row is left collapsing");
        require(ids()==QStringList{"r2","r3","h1","r1","d1"} && unsplit(),("after Down the kept row stays inside To review and no section repeats: "+ids().join(',')).toUtf8().constData());
        require(count("review")==2 && count("held")==1 && count("done:2026-09")==2,"section counts follow what is shown after Down");
        require(shownCount("mailGroup_review")==1 && shownCount("mailGroup_held")==1 && shownCount("mailSectionEmpty_review")==0,"one To review header, no empty state beside the kept row");
        // The last one: kept while Core's To review is 0, never 'All reviewed' beside it.
        press(Qt::Key_Down);
        read=h.take(); require(read.request["params"].toObject()["id"]=="r3","Down opens the last To review item");
        set("r3","done",false,false); order({"h1","r1","r2","r3","d1"}); counts(0,1,4); h.finish(read);
        await([&]{return !h.backend.m_mutatingConversation && !h.backend.m_loading && ids().first()=="r3";},"last kept after Down");
        settle();
        require(ids()==QStringList{"r3","h1","r1","r2","d1"} && unsplit(),("the last kept row stays above Delayed: "+ids().join(',')).toUtf8().constData());
        require(count("review")==1 && shownCount("mailGroup_review")==1 && shownCount("mailSectionEmpty_review")==0,"the last kept row shows To review 1, not 'All reviewed'");
        // Every other row in a group is shaded, starting again in each group.
        {
            const auto shade=[&](const QString &id){ auto *item=find(window->contentItem(),"rowHighlight_"+id); require(item,"row highlight exists"); return item->property("color").value<QColor>().alphaF(); };
            require(shade("r1")==0 && qAbs(shade("r2")-.028)<.005 && shade("d1")==0 && shade("h1")==0,"Done shades its second row; Delayed's first row is plain");
        }
        // Drafts in the corner follow Settings' Show/Hide.
        {
            h.backend.m_draftChips={{"account",h.backend.m_account},{"rows",QVariantList{QVariantMap{{"id","draft-1"},{"subject","Unsent note"},{"to_recipients",QVariantList{}}}}},{"total",1}};
            emit h.backend.draftChipsChanged();
            auto *chips=find(window->contentItem(),"draftChips"); require(chips,"draft chips exist");
            await([&]{return chips->isVisible();},"drafts show in the corner by default");
            h.backend.m_preferences["show_draft_chips"]=false; emit h.backend.preferencesChanged();
            await([&]{return !chips->isVisible();},"Hide takes the drafts out of the corner");
            h.backend.m_preferences["show_draft_chips"]=true; emit h.backend.preferencesChanged();
            await([&]{return chips->isVisible();},"Show brings them back");
            h.backend.m_draftChips.clear(); emit h.backend.draftChipsChanged();
        }
        // Delay it: it moves to Delayed at once and the view says all reviewed.
        h.backend.applyMailSelection(h.backend.snapshotMailRows({"r3"}),"delay");
        auto delay=h.take(); require(delay.request["params"].toObject()["starred"]==true,"Delay sends the hold");
        set("r3","held",false,true); order({"h1","r3","r1","r2","d1"}); counts(0,2,3); h.finish(delay);
        await([&]{return !h.backend.m_mutatingConversation && !h.backend.m_loading && ids().indexOf("r3")==1;},"Delay moves the open conversation to Delayed");
        settle();
        require(!h.backend.conversations()[1].toMap()["pinned"].toBool() && h.backend.conversations()[1].toMap()["section"]=="held" && unsplit(),"a delayed conversation is not kept in To review");
        require(count("review")==0 && count("held")==2 && shownCount("mailSectionEmpty_review")==1 && shownCount("mailGroup_review")==1,"after Delay To review shows its empty state once");
        require(h.backend.selectedId()=="r3","Delay keeps the conversation open");
        // Down goes on from where it was (h1), not from its new place (r1).
        press(Qt::Key_Down);
        await([&]{return h.backend.selectedId()=="h1";},"Down after Delay continues from the conversation's old place");
        settle();
        // Search matches are tinted where Core's phrase rules find them.
        require(h.backend.matchRanges("Bill\nC-22's, bill c-225 and xbill c-22",{"bill c-22"})==QVariantList{QVariant(QVariantList{0,9}),QVariant(QVariantList{13,22})},"highlights follow the phrase rules: case, line breaks and word starts");
        require(h.backend.matchRanges(QString::fromUtf8("Caf\u00e9  \u00c9lan"),{"cafe elan"})==QVariantList{QVariant(QVariantList{0,10})},"highlights ignore accents and spacing");
        require(h.backend.matchRanges("Sep 22",{"c-22"}).isEmpty(),"no highlight without the phrase");
        h.backend.m_conversation["subject"]="Review h1"; emit h.backend.conversationChanged();
        h.backend.m_searchTerms=QVariantMap{{"text","Review h1 body:retained"},{"phrases",QVariantList{QVariantMap{{"text","Review h1"},{"field","any"}},QVariantMap{{"text","retained"},{"field","body"}}}}};
        emit h.backend.searchTermsChanged();
        await([&]{auto *marks=find(window->contentItem(),"mailSubjectMatch_h1");return marks && marks->isVisible() && marks->property("ranges").toList().size()==1;},"the list subject tints its match");
        {
            auto *r1=find(window->contentItem(),"mailSubjectMatch_r1");
            require(!r1 || !r1->isVisible(),"a subject without the phrase has no tint");
            auto *subject=find(window->contentItem(),"readerSubject");
            await([&]{return subject->property("textFormat").toInt()==Qt::RichText && subject->property("text").toString().contains("background-color");},"the reader subject tints its match");
            QQuickItem *body=nullptr;
            // The text body and the (hidden) HTML view share the name; take the one shown.
            std::function<QQuickItem*(QQuickItem*)> visibleBody=[&](QQuickItem *item)->QQuickItem* {
                if(item->objectName()=="messageBody_h1-original" && item->isVisible()) return item;
                for(auto *child:item->childItems()) if(auto *found=visibleBody(child)) return found;
                return nullptr;
            };
            await([&]{body=visibleBody(window->contentItem());return body;},"the message body shows");
            if (body->property("highlightCount").isValid()) await([&]{return body->property("highlightCount").toInt()>0;},"the HTML body tints its match");
            else {
                auto *document=body->findChild<QTextDocument*>(QString(),Qt::FindDirectChildrenOnly); require(document,"the text body has a document");
                await([&]{for (auto block=document->begin();block!=document->end();block=block.next()) if (block.layout() && !block.layout()->formats().isEmpty()) return true; return false;},"the text body tints its match");
            }
            // A long subject wraps at the same line height with its tint.
            const auto longSubject=QString("Review h1 with a subject long enough to wrap onto a second line in the reader heading, Review h1 again");
            h.backend.m_conversation["subject"]=longSubject; emit h.backend.conversationChanged();
            await([&]{return subject->property("text").toString().contains("again");},"long subject shows");
            settle();
            const auto markedHeight=subject->height();
            if (qEnvironmentVariableIsSet("FM_SELECTION_CAPTURE")) window->grabWindow().save(qEnvironmentVariable("FM_SELECTION_CAPTURE"));
            h.backend.m_searchTerms.clear(); emit h.backend.searchTermsChanged();
            await([&]{return subject->property("textFormat").toInt()==Qt::PlainText && !find(window->contentItem(),"mailSubjectMatch_h1")->isVisible();},"clearing the search removes the tints");
            settle();
            qInfo()<<"subject heights"<<markedHeight<<subject->height();
            require(qAbs(markedHeight-subject->height())<1,"a tinted subject keeps the heading's line height");
        }
        // Changes waiting for or being checked with Google say nothing; a refusal
        // shows under the buttons, in a line kept for it, so nothing moves.
        {
            auto *note=find(window->contentItem(),"readerNote"); auto *actions=find(window->contentItem(),"conversationActionsRow");
            require(note && actions && note->isVisible() && actions->isVisible() && note->property("text").toString().isEmpty(),"an empty note line sits under the reader's buttons");
            const auto key=qMakePair(h.backend.m_account,h.backend.m_selected);
            const auto top=note->mapToScene({0,0}).y(); const auto height=note->height();
            for (const auto &state : {"pending","unknown"}) {
                h.backend.m_mailDelivery[key]={{"state",state}}; emit h.backend.mailDeliveryChanged(); settle();
                require(note->property("text").toString().isEmpty(),"pending and checking changes say nothing");
            }
            h.backend.m_mailDelivery[key]={{"state","failed"}}; emit h.backend.mailDeliveryChanged(); settle();
            require(note->property("text").toString().startsWith("Google did not accept this change"),"a refusal shows in the note line");
            require(qAbs(note->mapToScene({0,0}).y()-top)<.5 && qAbs(note->height()-height)<.5 && top>=actions->mapToScene({0,actions->height()}).y(),"the refusal sits under the buttons and moves nothing");
            require(!find(window->contentItem(),"mailDeliveryStatus") && !find(window->contentItem(),"messageDeliveryStatus"),"no status at the top of the reader");
            h.backend.m_mailDelivery.remove(key); emit h.backend.mailDeliveryChanged();
        }
        // Month and section headers have no tooltip.
        {
            auto *header=find(window->contentItem(),"mailGroup_review"); require(header,"To review header");
            const auto children=header->findChildren<QObject*>();
            require(std::none_of(children.cbegin(),children.cend(),[](QObject *child){return child->metaObject()->indexOfProperty("shown")>=0;}),"section headers carry no tooltip");
        }
        // Opening another while a row still collapses sends the first at once.
        {
            h.finish(h.take());
            await([&]{return !h.backend.m_mutatingConversation && !h.backend.m_loading;},"opening the delayed row settles");
            const auto rows=h.backend.conversations();
            h.backend.startLeaving(rows[1].toMap(),1); h.backend.startLeaving(rows[2].toMap(),2);
            const auto leaving=[&]{ QStringList out; for (const auto &row:h.backend.conversations()) if (row.toMap()["leaving"].toBool()) out.append(row.toMap()["id"].toString()); return out; };
            require(leaving()==QStringList{rows[2].toMap()["id"].toString()} && !ids().contains(rows[1].toMap()["id"].toString()),"only the latest row collapses; the earlier one goes at once");
            await([&]{return leaving().isEmpty() && !h.backend.m_loading && ids().size()==5;},"after the collapse the list settles with every row");
        }
        // The last conversation to review, read and then left, leaves Inbox
        // zero as soon as it has folded away, not when the refresh lands.
        {
            for (const auto &id : {"r1","r2","r3","h1","d1"}) set(id,"done",false,false);
            order({"r1","r2","r3","h1","d1"}); counts(0,0,5);
            h.backend.loadConversations(false,true); await([&]{return !h.backend.m_loading;},"every row is Done");
            auto rows=h.backend.conversations(); auto kept=rows[0].toMap();
            kept["pinned"]=true; kept["display_group"]="review"; kept["pinned_from"]="done"; rows[0]=kept;
            h.backend.publishConversations(rows); emit h.backend.conversationsChanged(); settle();
            require(shownCount("mailSectionEmpty_review")==0,"the read one still stands in To review");
            h.holdLists=true;
            h.backend.startLeaving(h.backend.conversations()[0].toMap(),0);
            QElapsedTimer folded; folded.start(); while(folded.elapsed()<330) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(shownCount("mailSectionEmpty_review")==1,"Inbox zero shows once the last row has folded away");
            h.holdLists=false; h.releaseLists(); await([&]{return !h.backend.m_loading;},"the refresh after it lands");
            require(shownCount("mailSectionEmpty_review")==1,"and stays when the refresh lands");
        }
        // Collapsing the only loaded month while older mail isn't loaded keeps
        // the headers, and refreshes never show a loading text.
        {
            for (const auto &id : {"r1","r2","r3","h1","d1"}) set(id,"done",false,false);
            order({"r3","h1","r1","r2","d1"}); counts(0,0,5);
            h.backend.loadConversations(false,true); await([&]{return !h.backend.m_loading;},"every row in one month");
            settle();
            auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"mail workspace");
            // Neither a refresh nor paging older mail says Loading.
            const auto loadingText=[&]{ bool found=false; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){
                const auto text=item->property("text").toString();
                if (item->isVisible() && (text==QStringLiteral("Searching\u2026") || text==QStringLiteral("Loading\u2026"))) found=true;
                for (auto *child:item->childItems()) walk(child); };
                walk(window->contentItem()); return found; };
            // Like Core, the refresh leaves the collapsed month's rows out.
            const auto month=h.rows; h.rows.clear();
            h.backend.m_nextOffset=5;
            h.holdLists=true;
            h.backend.setSectionCollapsed("done:2026-09",true);
            // A folded section keeps one hidden host row; nothing is shown.
            require(h.backend.m_loading && shownRows(h.backend)==0 && !mail->property("firstLoading").toBool(),"collapsing empties the loaded rows while the refresh runs");
            // The next frame shows the headers, while the refresh is still out.
            settle();
            require(h.backend.m_loading && shownCount("mailGroup_review")==1 && shownCount("mailGroup_done:2026-09")==1 && !loadingText(),"the headers stay and nothing says Searching or Loading");
            h.holdLists=false; h.releaseLists();
            await([&]{return !h.backend.m_loading;},"the refresh after collapsing lands");
            require(shownCount("mailGroup_done:2026-09")==1 && !loadingText(),"no loading text after the refresh");
            h.rows=month;
            h.backend.setSectionCollapsed("done:2026-09",false);
            require(shownRows(h.backend)==5 && !loadingText(),"expanding puts the month's rows back at once with no loading text");
            await([&]{return !h.backend.m_loading;},"the refresh after expanding lands");
        }
    }
    static void undoActions() {
        Harness h;
        QVariantList restored;
        QObject::connect(&h.backend,&MailBackend::mailSelectionRestored,[&](const auto &rows){restored=rows;});
        h.backend.applyMailSelection(h.backend.snapshotMailRows({"a","b"}),"done");
        for(int i=0;i<2;++i) {
            const auto held=h.take(); const auto p=held.request["params"].toObject();
            held.peer->write(reply(held.request["id"].toString(),{{"account","personal"},{"id",p["id"]},{"revision",4},{"message_ids",p["message_ids"]},{"messages",QJsonArray{}},{"mail_undo",QJsonObject{{"version",1},{"request_id",p["request_id"]},{"revision",4}}}}));
        }
        await([&]{return !h.backend.m_mutatingConversation;},"bulk action completes before undo");
        require(h.backend.actionToast().value("action")=="undo_mail" && h.backend.actionToast().value("duration")==6000,"bulk mutation offers six-second Undo");
        h.backend.activateActionToast();
        for(int i=0;i<2;++i) {
            const auto held=h.take();const auto p=held.request["params"].toObject();
            require(held.request["method"]=="mail_undo" && p["expected_revision"]==4 && !p.contains("message_ids"),"Undo uses frozen receipt and revision, not guessed inverse flags");
            held.peer->write(reply(held.request["id"].toString(),{{"account","personal"},{"id",p["id"]},{"revision",5},{"undone_request_id",p["undo_request_id"]},{"messages",QJsonArray{QJsonObject{{"id",p["id"].toString()+"-original"},{"unread",true},{"starred",false}}}}}));
        }
        await([&]{return !h.backend.m_mutatingConversation && restored.size()==2;},"Undo restores full bulk selection");
        require(restored[0].toMap().value("revision")==5 && restored[0].toMap().value("unread").toBool(),"restored selection uses returned revision and review state");
        h.backend.offerMailUndo("Changed",{QVariantMap{{"account","personal"}}},{});
        h.backend.dismissMailUndo();require(!h.backend.actionToast().value("visible").toBool(),"opening another conversation dismisses stale Undo");
    }
    static QQuickItem *find(QQuickItem *item,const QString &name) {
        if(item->objectName()==name) return item;
        for(auto *child:item->childItems()) if(auto *found=find(child,name)) return found;
        return nullptr;
    }
    // The painted match: the HTML reader and its text fallback share a name.
    static QQuickItem *shown(QQuickItem *item,const QString &name) {
        if(!item->isVisible()) return nullptr;
        if(item->objectName()==name) return item;
        for(auto *child:item->childItems()) if(auto *found=shown(child,name)) return found;
        return nullptr;
    }
    static void click(QQuickWindow *window,QQuickItem *item,Qt::MouseButton button,Qt::KeyboardModifiers modifiers={}) {
        if (!item || !item->isVisible()) qInfo()<<"hidden target"<<(item ? item->objectName() : "missing");
        require(item && item->isVisible(),"actual selection target visible");
        const auto p=item->mapToScene(item->boundingRect().center());
        QMouseEvent move(QEvent::MouseMove,p,window->mapToGlobal(p.toPoint()),Qt::NoButton,Qt::NoButton,modifiers);
        QCoreApplication::sendEvent(window,&move);
        QMouseEvent down(QEvent::MouseButtonPress,p,window->mapToGlobal(p.toPoint()),button,button,modifiers);
        QCoreApplication::sendEvent(window,&down);
        QMouseEvent up(QEvent::MouseButtonRelease,p,window->mapToGlobal(p.toPoint()),button,Qt::NoButton,modifiers);
        QCoreApplication::sendEvent(window,&up);
    }
    static void controls() {
        Harness h;
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings=false;
        QObject::connect(&qml,&QQmlEngine::warnings,[&](const auto &errors){warnings=true;for(const auto &error:errors) qWarning()<<error.toString();});
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        // The list/reader divider is one app-wide width kept with the window's place.
        QTemporaryDir layoutProfile; require(layoutProfile.isValid(),"layout profile");
        WindowPlacement splitLayout(layoutProfile.path(), {1000,650}); splitLayout.setMailSplitWidth(520);
        qml.rootContext()->setContextProperty("windowLayout",&splitLayout);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; function present(theme,size) { Theme.themeId=theme; Theme.textSize=size } MailView { anchors.fill:parent } StartupOverlay { id:splash; anchors.fill:parent; startupReady:true } }",QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!qml.rootObjects().isEmpty(),"actual MailView and splash load");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"selection window exposed");
        auto *splash=find(window->contentItem(),"startupOverlay"); require(splash,"real splash exists");
        QElapsedTimer elapsed; elapsed.start();
        while(elapsed.elapsed()<1450) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        require(!splash->property("dismissed").toBool() && splash->opacity()==1,"fast startup displays complete lettering before fade");
        await([&]{return splash->property("dismissed").toBool();},"splash eventually releases input");
        require(elapsed.elapsed()>=1700,"minimum splash hold respected");
        {
            auto pane=find(window->contentItem(),"mailListPane"); require(pane,"list pane");
            await([&]{return qAbs(pane->width()-520)<1;},"the saved divider applies at startup");
            // An account's old per-account width no longer moves the divider.
            auto views=h.backend.m_views; auto view=views.value("personal").toMap(); view["splitWidth"]=900; views["personal"]=view; h.backend.m_views=views;
            emit h.backend.presentationChanged();
            QElapsedTimer settle; settle.start(); while(settle.elapsed()<150) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(qAbs(pane->width()-520)<1,"view state leaves the divider alone");
            // Dragging the divider saves one width when the drag ends.
            const QPoint from=pane->mapToScene(QPointF(pane->width(),pane->height()/2)).toPoint(), to=from+QPoint(80,0);
            QTest::mousePress(window,Qt::LeftButton,Qt::NoModifier,from);
            for(int step=1;step<=8;++step){QTest::mouseMove(window,from+QPoint(step*10,0));QCoreApplication::processEvents();}
            require(qAbs(splitLayout.mailSplitWidth()-520)<1,"nothing is saved mid-drag");
            QTest::mouseRelease(window,Qt::LeftButton,Qt::NoModifier,to);
            await([&]{return qAbs(splitLayout.mailSplitWidth()-pane->width())<1 && pane->width()>560;},"the dragged width is saved on release");
        }
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"actual mail workspace");
        auto summaryRow=h.rows[0].toMap(); summaryRow["summary"]="Review the design by Friday."; summaryRow["snippet"]="Original body preview";
        summaryRow["others"]=2; summaryRow["other_names"]=QVariantList{"Ann","Bob"}; summaryRow["message_count"]=3; summaryRow["attachment_count"]=1; h.rows[0]=summaryRow;
        h.backend.publishConversations(h.rows); emit h.backend.conversationsChanged();
        await([&]{auto *preview=find(window->contentItem(),"mailPreview_a");return preview && preview->property("text")=="Review the design by Friday.";},"summary replaces source preview");
        {
            auto *others=shown(window->contentItem(),"mailOthers_a"), *thread=shown(window->contentItem(),"mailThreadCount_a"), *attachments=shown(window->contentItem(),"mailAttachmentCount_a");
            require(others && others->property("text")=="+2","other people on the thread follow the sender");
            require(thread && attachments && attachments->x()>thread->x()+thread->width(),"thread and attachment counts follow the subject in order");
            require(!find(window->contentItem(),"mailOthers_b")->isVisible() && !find(window->contentItem(),"mailThreadCount_b")->isVisible(),"single-person, single-message rows show neither");
        }
        require(find(window->contentItem(),"summaryPreviewIcon_a")->isVisible(),"summary preview has assistance icon");
        auto *senderColumn=find(window->contentItem(),"mailSenderColumn_a");
        auto *senderAddress=find(window->contentItem(),"mailSenderAddress_a");
        auto *sourcePreview=find(window->contentItem(),"mailPreview_a");
        require(senderColumn && senderColumn->isVisible() && senderAddress->property("text")=="sender@example.test","dedicated sender name/address column");
        require(senderColumn->mapToScene(QPointF(senderColumn->width(),0)).x() < sourcePreview->mapToScene(QPointF()).x(),"sender column precedes subject/preview without overlap");
        {
            auto *avatar=shown(window->contentItem(),"mailAvatar_a");
            require(avatar && qFuzzyCompare(avatar->width(),28.) && qAbs(senderColumn->mapToScene(QPointF()).x()-avatar->mapToScene(QPointF(avatar->width(),0)).x()-12)<.5,"a 28px sender avatar sits 12px before the sender column");
            require(avatar->property("initials").toString()=="S" && !avatar->property("imageShown").toBool() && avatar->property("sourceLabel").toString()==QString::fromUtf8("No photo · initials"),"a synthetic profile shows initials and asks for no picture");
        }
        summaryRow["recipient"]="teammate@example.test";h.rows[0]=summaryRow;h.backend.publishConversations(h.rows); emit h.backend.conversationsChanged();
        h.backend.m_folder="sent";emit h.backend.queryChanged();
        await([&]{auto recipient=find(window->contentItem(),"mailRecipient_a"), item=find(window->contentItem(),"mailPreview_a");return recipient && recipient->isVisible() && recipient->property("text").toString()=="To teammate@example.test" && item && item->property("text").toString().startsWith("· ");},"Sent preview identifies its recipient");
        require(!find(window->contentItem(),"mailSenderColumn_a")->isVisible() && !find(window->contentItem(),"mailAvatar_a")->isVisible(),"Sent retains the prototype subject/recipient arrangement");
        h.backend.m_folder="inbox";emit h.backend.queryChanged();
        require(!find(window->contentItem(),"threadSummary") && !find(window->contentItem(),"summarizeConversation"),"no redundant thread summary surface");
        summaryRow.remove("summary");h.rows[0]=summaryRow;h.backend.publishConversations(h.rows); emit h.backend.conversationsChanged();
        await([&]{auto item=find(window->contentItem(),"mailPreview_a");return item && item->property("text").toString().contains("Original body preview");},"missing summary falls back to source text");
        require(!find(window->contentItem(),"summaryPreviewIcon_a")->isVisible(),"source preview has no assistance icon");
        click(window,find(window->contentItem(),"conversationRow_a"),Qt::RightButton);
        await([&]{auto item=find(window->contentItem(),"contextDone");return item && item->isVisible();},"right click opens actual menu");
        require(h.writes.isEmpty() && !h.methods.contains("conversation"),"right click is not deliberate open/read");
        QKeyEvent escape(QEvent::KeyPress,Qt::Key_Escape,Qt::NoModifier); QCoreApplication::sendEvent(window,&escape);
        click(window,find(window->contentItem(),"conversationRow_a"),Qt::LeftButton,Qt::ControlModifier);
        click(window,find(window->contentItem(),"conversationRow_c"),Qt::LeftButton,Qt::ShiftModifier);
        require(mail->property("selectionRows").toList().size()==3,"Ctrl click plus Shift range selects three without opening");
        require(h.writes.isEmpty(),"multi-selection never marks read");
        click(window,find(window->contentItem(),"conversationRow_b"),Qt::RightButton);
        await([&]{auto item=find(window->contentItem(),"contextDone");return item && item->isVisible();},"bulk context menu");
        require(!find(window->contentItem(),"contextReply")->isVisible(),"reply unavailable for multiple conversations");
        click(window,find(window->contentItem(),"contextDone"),Qt::LeftButton);
        for(int i=0;i<3;++i) h.finish(h.take());
        await([&]{return !h.backend.m_mutatingConversation;},"native bulk Done completes");
        require(h.backend.mailSelectionStatus().isEmpty(),"a finished selection action leaves no status line");
        h.backend.loadCategories();
        await([&]{return !h.backend.categories().isEmpty();},"label catalog available");
        click(window,find(window->contentItem(),"conversationRow_a"),Qt::RightButton);
        click(window,find(window->contentItem(),"contextLabels"),Qt::LeftButton);
        await([&]{auto *item=find(window->contentItem(),"applyLabel_imported");return item && item->isVisible();},"label submenu opens");
        QElapsedTimer layout; layout.start(); while(layout.elapsed()<100) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"applyLabel_imported"),Qt::LeftButton);
        auto label=h.take();
        require(label.request["method"]=="set_category" && label.request["params"].toObject()["category_id"]=="imported" && label.request["params"].toObject()["present"]==false,"checked submenu label removes exactly that label from conversations that all carry it");
        h.finish(label); await([&]{return !h.backend.m_mutatingConversation;},"label action completes");
        QMetaObject::invokeMethod(mail,"clearSelection");
        mail->setProperty("selectionAnchor","a");
        h.backend.m_selected="a";
        find(window->contentItem(),"conversationRow_a")->forceActiveFocus();
        h.backend.m_preferences["shortcuts"]=QVariantMap{{"next","X"}};emit h.backend.preferencesChanged();
        QKeyEvent oldNext(QEvent::KeyPress,Qt::Key_J,Qt::NoModifier);QCoreApplication::sendEvent(window,&oldNext);
        QCoreApplication::processEvents(QEventLoop::AllEvents,30);
        require(h.backend.m_selected=="a","old navigation key stops after rebinding");
        QKeyEvent newNext(QEvent::KeyPress,Qt::Key_X,Qt::NoModifier);QCoreApplication::sendEvent(window,&newNext);
        await([&]{return h.backend.m_selected=="b";},"replacement shortcut actually navigates");
        h.backend.m_preferences.remove("shortcuts");emit h.backend.preferencesChanged();
        click(window,find(window->contentItem(),"conversationRow_a"),Qt::LeftButton);
        QMetaObject::invokeMethod(mail,"clearSelection");mail->setProperty("selectionAnchor","a");
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
        await([&]{auto item=find(window->contentItem(),"trashConversation");return item && item->isVisible();},"Trash context visible");
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
        require(!find(window->contentItem(),"conversationRow_a") || !find(window->contentItem(),"conversationRow_a")->isVisible(),"date group collapses");
        // Sent and Drafts are always grouped; an old plain-list choice changes nothing.
        h.backend.m_preferences["mail_date_grouping"]="plain"; emit h.backend.preferencesChanged();
        require(mail->property("dateGroups").toBool(),"a retired plain-list preference keeps the date groups");
        click(window,header,Qt::LeftButton);
        await([&]{auto item=find(window->contentItem(),"conversationRow_a");return item && item->isVisible();},"the date group opens again");
        h.backend.m_folder="inbox"; h.backend.m_selected="a"; emit h.backend.queryChanged();
        h.backend.m_conversation={{"id","a"},{"subject","Source inspection"},{"revision",3},{"message_ids",QVariantList{"a-original"}},{"messages",QVariantList{QVariantMap{{"id","a-original"},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text","Retained plain body"},{"source_available",true},{"source_version",QString(64, char(97))},{"sent",false}}}}};
        emit h.backend.conversationChanged();
        // Plain text until the sender is chosen for HTML; the badge chooses it for all their mail.
        await([&]{auto item=find(window->contentItem(),"messageFormatToggle_a-original");return item && item->isVisible() && item->property("text")=="TEXT";},"plain text by default once the HTML is known");
        require(shown(window->contentItem(),"messageBody_a-original")->property("text").toString().contains("Retained plain body"),"the default body is the extracted text");
        require(find(window->contentItem(),"messageFormatToggle_a-original")->property("tip").toString().endsWith("for all mail from sender@example.test"),"the badge names the sender it changes");
        click(window,find(window->contentItem(),"messageFormatToggle_a-original"),Qt::LeftButton);
        {
            const auto chosen=h.backend.preferences().value("html_senders").toList();
            require(chosen.size()==1 && chosen.first().toMap().value("account")=="personal" && chosen.first().toMap().value("email")=="sender@example.test","HTML is saved for this sender in this account");
        }
        await([&]{auto item=shown(window->contentItem(),"messageBody_a-original");return item && item->property("text").toString().contains("Retained HTML");},"received HTML loads for the expanded message");
        require(h.writes.isEmpty(),"source inspection queues no mutations");
        auto body=shown(window->contentItem(),"messageBody_a-original")->property("text").toString();
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
        await([&]{auto item=shown(window->contentItem(),"messageBody_a-original");return item && item->property("text").toString().contains("Retained plain body");},"plain format uses retained extracted body");
        require(h.backend.preferences().value("html_senders").toList().isEmpty(),"choosing plain text forgets the sender's HTML choice");
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
        const QVariantList toSelf{QVariantMap{{"label","To"},{"people",QVariantList{QVariantMap{{"name","You"},{"email","test@example.test"}}}}}};
        auto firstMessage=QVariantMap{{"id","first"},{"sender_name","Priya Natarajan"},{"sender_email","priya@example.test"},{"sent_at","2026-09-14T09:30:00Z"},{"text","Draft release notes are ready for review."},{"sent",false},{"kind","people"},{"metadata",toSelf}};
        auto selfMessage=QVariantMap{{"id","self"},{"sender_name","Test User"},{"sender_email","test@example.test"},{"sent_at","2026-09-14T12:05:00Z"},{"text","Lead with reconnect. Keep it short."},{"sent",true},{"metadata",QVariantList{QVariantMap{{"label","To"},{"people",QVariantList{QVariantMap{{"name","Priya Natarajan"},{"email","priya@example.test"}}}}}}}};
        auto lastMessage=firstMessage; lastMessage["id"]="last"; lastMessage["sent_at"]="2026-09-16T11:24:00Z"; lastMessage["text"]="Final release notes are ready. Ships Tuesday morning; the changelog follows an hour later.\n\nOn Tuesday, Alex <alex@example.test> wrote:\n> Earlier reply\n>> Oldest reply"; lastMessage["starred"]=true;
        auto metadata=toSelf; metadata.append(QVariantMap{{"label","Cc"},{"people",QVariantList{QVariantMap{{"name","Team"},{"email","team@example.test"}}}}}); lastMessage["metadata"]=metadata;
        h.backend.m_conversation={{"id","a"},{"subject","Router firmware 1.8 — release notes"},{"categories",QVariantList{QVariantMap{{"id","apps"},{"name","Apps"}}}},{"messages",QVariantList{firstMessage,selfMessage,lastMessage}}};
        emit h.backend.conversationChanged(); mail->setProperty("expanded",true);
        QMetaObject::invokeMethod(mail,"toggleMember",Q_ARG(QVariant,QString("first")),Q_ARG(QVariant,0));
        QMetaObject::invokeMethod(mail,"scrollToMember",Q_ARG(QVariant,QString("first")));
        layout.restart();while(layout.elapsed()<160)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        auto sender=find(window->contentItem(),"senderFilter_last");
        auto kind=find(window->contentItem(),"messageSenderKind_last");
        require(sender && kind && kind->isVisible(),"sender identity and correction icon visible");
        const auto earlyCapture=qEnvironmentVariable("FM_READER_CAPTURE"); if(!earlyCapture.isEmpty())require(window->grabWindow().save(earlyCapture),"reader layout diagnostic");
        require(qAbs(kind->mapToScene({0,0}).x()-sender->mapToScene({sender->width(),0}).x()-6)<2,"kind icon sits six pixels after the name");
        require(!find(window->contentItem(),"messageFullDate_last"),"full timestamp is confined to the hover tooltip");
        require(!find(window->contentItem(),"recipientName_last_test@example.test"),"redundant received To-self line omitted");
        auto verifyDateMenu=[&] {
            for (const auto &id: {"first", "last"}) {
                auto header=find(window->contentItem(),QString("messageHeading_")+id);
                auto menu=find(window->contentItem(),QString("messageMore_")+id);
                auto date=find(window->contentItem(),QString("messageShortDate_")+id);
                auto chevron=find(window->contentItem(),QString("toggleMessage_")+id);
                require(header && menu && date && chevron,"single-line header controls exist");
                require(qAbs(header->height()-44)<1,"expanded header remains exactly 44 pixels");
                require(qAbs(menu->height()-26)<1 && qAbs(chevron->height()-26)<1,"borderless header controls remain 26 pixels");
                require(date->mapToScene({date->width(),0}).x()<menu->mapToScene({0,0}).x() && menu->mapToScene({menu->width(),0}).x()<chevron->mapToScene({0,0}).x(),"time, menu, chevron order matches design");
                require(qAbs(chevron->mapToItem(header,{chevron->width(),0}).x()-header->width())<1,"chevron ends at reader right edge");
            }
        };
        verifyDateMenu();
        const auto localInstant=QDateTime(QDate(2026,9,11),QTime(9,12),QTimeZone::systemTimeZone());
        const auto formatted=[&](const QString &value) {
            QVariant result;require(QMetaObject::invokeMethod(mail,"fullTime",Q_RETURN_ARG(QVariant,result),Q_ARG(QVariant,value)),"date formatter available");return result.toString();
        };
        require(formatted(localInstant.toUTC().toString(Qt::ISODate))=="Friday 11 September 2026, 09:12","full date uses the actual local weekday/date and 24-hour time without a zone suffix");
        require(formatted(localInstant.toOffsetFromUtc(9*3600).toString(Qt::ISODate))==formatted(localInstant.toUTC().toString(Qt::ISODate)),"equivalent timezone-offset instants display identically");
        const auto localMidnight=QDateTime(QDate(2026,9,12),QTime(0,3),QTimeZone::systemTimeZone());
        require(formatted(localMidnight.toUTC().toString(Qt::ISODate))=="Saturday 12 September 2026, 00:03","date follows local midnight rather than the source UTC calendar date");
        require(!find(window->contentItem(),"quotedReply_last_1") && !find(window->contentItem(),"messageBodyPart_last_5"),"quoted history creates neither controls nor hidden body delegates");
        require(find(window->contentItem(),"messageBody_last")->property("text").toString().contains("Final release notes are ready"),"thread reader retains the new contribution");
        {
            // Reply shows the whole reply at the end of the thread, and keeps it
            // there while the thread above it grows.
            auto *reader=find(window->contentItem(),"reader"); require(reader,"reader");
            const auto thread=h.backend.m_conversation;
            const int windowHeight=window->height(); window->setHeight(560);
            layout.restart();while(layout.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            reader->setProperty("contentY",0);
            QMetaObject::invokeMethod(mail,"openReply");
            layout.restart();while(layout.elapsed()<60)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            auto grown=thread; auto messages=grown["messages"].toList(); auto later=messages.first().toMap();
            later["id"]="later"; later["text"]=QString("A longer message that lays out after Reply.\n").repeated(40); messages.insert(1,later); grown["messages"]=messages;
            h.backend.m_conversation=grown; emit h.backend.conversationChanged();
            auto *reveal=window->findChild<QObject*>("replyReveal"); require(reveal,"reply reveal");
            await([&]{return !reveal->property("running").toBool();},"the reply finishes coming into view");
            auto *panel=find(window->contentItem(),"replyPanel"); require(panel && panel->isVisible() && panel->height()>0,"Reply opens the composer");
            const auto bottom=qMax(0.0,reader->property("contentHeight").toDouble()-reader->height());
            require(reader->property("contentHeight").toDouble()>reader->height()+40,"the thread is longer than the reader");
            const auto footer=panel->mapToItem(reader,QPointF(0,panel->height())).y();
            if (qAbs(reader->height()-footer-24)>=1 && qAbs(reader->property("contentY").toDouble()-bottom)>=1) qWarning()<<"reply footer at"<<footer<<"of"<<reader->height();
            require(qAbs(reader->height()-footer-24)<1 || qAbs(reader->property("contentY").toDouble()-bottom)<1,"Reply brings its footer into view with 24px to spare");
            const auto top=panel->mapToItem(reader,QPointF(0,0)).y(), end=panel->mapToItem(reader,QPointF(0,panel->height())).y();
            require(top>=-1 && end<=reader->height()+1,"the whole reply shows");
            QMetaObject::invokeMethod(mail,"setReplyOpen",Q_ARG(QVariant,false));
            h.backend.m_conversation=thread; emit h.backend.conversationChanged(); window->setHeight(windowHeight);
            find(window->contentItem(),"conversationList")->forceActiveFocus();
            layout.restart();while(layout.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        }
        auto popup=[&]()->QObject* {for(auto object:window->findChildren<QObject *>("senderCard"))if(object->property("opened").toBool())return object;return nullptr;};
        auto move=[&](const QPointF &p) {QMouseEvent event(QEvent::MouseMove,p,window->mapToGlobal(p.toPoint()),Qt::NoButton,Qt::NoButton,Qt::NoModifier);QCoreApplication::sendEvent(window,&event);};
        require(!find(window->contentItem(),"participantAvatar_self"),"thread summary lists the other people, not the user");
        for(const auto &[target,address]:QList<QPair<QString,QString>>{{"participantName_first","priya@example.test"},{"senderAvatar_self","test@example.test"},{"senderFilter_self","test@example.test"},{"senderAvatar_first","priya@example.test"},{"recipientName_last_team@example.test","team@example.test"}}) {
            auto item=find(window->contentItem(),target);require(item && item->isVisible(),qPrintable("each contact target is visible: "+target));
            move(item->mapToScene(item->boundingRect().center()));
            await([&]{return popup() && popup()->property("email").toString()==address;},"name/avatar hover opens exact contact card");
            QMetaObject::invokeMethod(popup(),"close"); move({5,5});
        }
        require(!find(window->contentItem(),"messageBody_self") || !find(window->contentItem(),"messageBody_self")->isVisible(),"hovering own name leaves sent row folded");
        require(find(window->contentItem(),"recipientName_last_team@example.test")->property("tip").toString().isEmpty(),"recipient names have no address tooltip beside the contact card");
        {
            // The card's text can be selected and copied; while it holds a selection
            // the card stays open after the pointer leaves.
            auto *name=find(window->contentItem(),"participantName_first");
            move(name->mapToScene(name->boundingRect().center()));
            await([&]{return popup() && popup()->property("email")=="priya@example.test" && popup()->property("opened").toBool();},"card opens for copying");
            auto *cardItem=qvariant_cast<QQuickItem *>(popup()->property("contentItem"));
            QQuickItem *address=nullptr;
            await([&]{address=find(cardItem,"senderCardEmail");return address && address->isVisible() && address->width()>0;},"card address shows");
            require(address->property("selectByMouse").toBool() && address->property("readOnly").toBool() && find(cardItem,"senderCardName")->property("selectByMouse").toBool()
                && find(cardItem,"senderCardStatus")->property("selectByMouse").toBool(),"card name, address and status are selectable");
            const qreal end=qMin(address->property("contentWidth").toReal(),address->width())-1;
            const auto from=address->mapToScene({1,address->height()/2}), to=address->mapToScene({end,address->height()/2});
            auto mouse=[&](QEvent::Type type,const QPointF &p,Qt::MouseButton button,Qt::MouseButtons buttons){QMouseEvent event(type,p,window->mapToGlobal(p.toPoint()),button,buttons,Qt::NoModifier);QCoreApplication::sendEvent(window,&event);};
            mouse(QEvent::MouseButtonPress,from,Qt::LeftButton,Qt::LeftButton);
            for(int step=1;step<=8;++step) mouse(QEvent::MouseMove,from+(to-from)*step/8.,Qt::NoButton,Qt::LeftButton);
            mouse(QEvent::MouseButtonRelease,to,Qt::LeftButton,Qt::NoButton);
            await([&]{return address->property("selectedText").toString()=="priya@example.test";},"dragging selects the whole address");
            move({5,5});
            layout.restart();while(layout.elapsed()<500)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(popup() && popup()->property("holding").toBool(),"the card stays open while it holds a selection");
            QGuiApplication::clipboard()->clear();
            QTest::keyClick(window,Qt::Key_C,Qt::ControlModifier);
            await([&]{return QGuiApplication::clipboard()->text()=="priya@example.test";},"Ctrl+C copies the card's selection");
            QMetaObject::invokeMethod(popup(),"close");
            await([&]{return !popup();},"the card closes");
            require(address->property("selectedText").toString().isEmpty(),"closing the card clears its selection");
        }
        auto heading=find(window->contentItem(),"messageHeading_first");
        move(heading->mapToScene({heading->width()/2,heading->height()/2}));
        layout.restart();while(layout.elapsed()<260)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        require(!popup(),"empty expanded header area is not a contact target");
        click(window,heading,Qt::LeftButton);
        layout.restart();while(layout.elapsed()<120)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        require(find(window->contentItem(),"messageBody_first") && find(window->contentItem(),"messageBody_first")->isVisible(),"clicking an open header leaves it open, so its text can be selected");
        require(find(window->contentItem(),"messageSenderAddress_first") && find(window->contentItem(),"messageSenderAddress_first")->property("selectByMouse").toBool(),"an open header's address is selectable");
        click(window,find(window->contentItem(),"toggleMessage_first"),Qt::LeftButton);
        await([&]{auto body=find(window->contentItem(),"messageBody_first");return !body || !body->isVisible();},"the chevron collapses an earlier message");
        require(!heading->property("visualFocus").toBool(),"a mouse click leaves no focus outline");
        heading->forceActiveFocus();
        QKeyEvent expandDown(QEvent::KeyPress,Qt::Key_Space,Qt::NoModifier), expandUp(QEvent::KeyRelease,Qt::Key_Space,Qt::NoModifier);
        QCoreApplication::sendEvent(window,&expandDown);QCoreApplication::sendEvent(window,&expandUp);
        await([&]{auto body=find(window->contentItem(),"messageBody_first");return body && body->isVisible();},"Space expands the focused message header again");
        click(window,find(window->contentItem(),"toggleMessage_last"),Qt::LeftButton);
        await([&]{auto body=find(window->contentItem(),"messageBody_last");return !body || !body->isVisible();},"the newest message can collapse too");
        click(window,find(window->contentItem(),"toggleMessage_last"),Qt::LeftButton);
        await([&]{auto body=find(window->contentItem(),"messageBody_last");return body && body->isVisible();},"newest message re-expands");
        click(window,find(window->contentItem(),"messageHeading_self"),Qt::LeftButton);
        await([&]{auto item=find(window->contentItem(),"recipientName_self_priya@example.test");return item && item->isVisible();},"folded sent row expands with contactable recipient metadata");
        layout.restart();while(layout.elapsed()<120)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        click(window,find(window->contentItem(),"recipientName_self_priya@example.test"),Qt::LeftButton);
        await([&]{return popup() && popup()->property("email")=="priya@example.test";},"recipient activation opens contact card");
        QMetaObject::invokeMethod(popup(),"close"); move({5,5});
        click(window,find(window->contentItem(),"toggleMessage_self"),Qt::LeftButton);
        await([&]{auto body=find(window->contentItem(),"messageBody_self");return !body || !body->isVisible();},"expanded sent message collapses through its visible control");
        click(window,find(window->contentItem(),"earlierMessagesButton"),Qt::LeftButton);
        await([&]{auto header=find(window->contentItem(),"messageHeading_first");return !header || !header->isVisible();},"earlier-message group hides again");
        click(window,find(window->contentItem(),"earlierMessagesButton"),Qt::LeftButton);
        await([&]{auto header=find(window->contentItem(),"messageHeading_first");return header && header->isVisible();},"earlier-message group reopens");
        const auto readerCapture=qEnvironmentVariable("FM_READER_CAPTURE");
        if(!readerCapture.isEmpty()){layout.restart();while(layout.elapsed()<160)QCoreApplication::processEvents(QEventLoop::AllEvents,10);require(window->grabWindow().save(readerCapture),"prototype reader frame");}
        for(const auto &theme: {"paper","terminal","graphite"}) {
            const bool narrow=QString(theme)=="graphite";
            window->resize(narrow?960:1280,850);
            QMetaObject::invokeMethod(window,"present",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(narrow?"large":"default")));
            QMetaObject::invokeMethod(mail,"scrollToMember",Q_ARG(QVariant,QString("last")));
            layout.restart();while(layout.elapsed()<140)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            verifyDateMenu();
            auto date=find(window->contentItem(),"messageShortDate_last");
            auto lastHeader=find(window->contentItem(),"messageHeading_last");
            const auto rect=date->mapRectToItem(lastHeader,date->boundingRect());
            require(rect.left()>=0 && rect.right()<=lastHeader->width()+1 && rect.bottom()<=lastHeader->height()+1,"timestamp remains inside header at narrow large text and all themes");
            require(find(window->contentItem(),"senderFilter_self")->width()>15,"folded own name keeps a usable contact target");
            auto summaryAction=find(window->contentItem(),"summarizeMessage_last");
            require(summaryAction && summaryAction->width()<=lastHeader->width()-38,"summary control remains within the indented column");
            if(!readerCapture.isEmpty())require(window->grabWindow().save(readerCapture.chopped(4)+"-"+theme+".png"),"native theme/size reader frame");
        }
        window->resize(1280,800);
        QMetaObject::invokeMethod(window,"present",Q_ARG(QVariant,QString("graphite")),Q_ARG(QVariant,QString("default")));
        // Exercise actual attachment cards with synthetic local bytes, never a real mailbox.
        const auto pngPath=copy.filePath("landscape.png");
        QImage raster(1600,1000,QImage::Format_RGB32); raster.fill(QColor("#648899"));
        require(raster.save(pngPath),"synthetic image saved");
        const auto binaryPath=copy.filePath("archive.bin");
        QFile binary(binaryPath); require(binary.open(QIODevice::WriteOnly),"synthetic binary opens");
        const QByteArray binaryBytes(1024*1024+31,'x');require(binary.write(binaryBytes)==binaryBytes.size(),"multi-chunk binary written");binary.close();
        auto addAttachment=[&](const QString &id,const QString &path,const QString &mime) {
            QFile source(path);require(source.open(QIODevice::ReadOnly),"synthetic source opens");const auto bytes=source.readAll();
            h.attachments[id]={{"account","personal"},{"id",id},{"name",QFileInfo(path).fileName()},{"mime_type",mime},{"state","available"},{"path",path},{"size",bytes.size()},{"sha256",QString::fromLatin1(QCryptographicHash::hash(bytes,QCryptographicHash::Sha256).toHex())}};
        };
        addAttachment("image",pngPath,"image/png");addAttachment("binary",binaryPath,"application/octet-stream");
        // A real, self-contained Motion JPEG AVI exercises the installed media decoder.
        const auto aviLe=[](quint32 n){QByteArray b;for(int i=0;i<4;++i)b.append(char((n>>(8*i))&255));return b;};
        const auto aviChunk=[&](const QByteArray &tag,const QByteArray &payload){return tag+aviLe(quint32(payload.size()))+payload+(payload.size()%2 ? QByteArray(1,'\0') : QByteArray{});};
        const auto aviWords=[&](std::initializer_list<quint32> values){QByteArray b;for(auto value:values)b+=aviLe(value);return b;};
        QImage videoFrame(80,48,QImage::Format_RGB32);videoFrame.fill(QColor("#94659e"));QByteArray videoJpeg;QBuffer jpegBuffer(&videoJpeg);jpegBuffer.open(QIODevice::WriteOnly);require(videoFrame.save(&jpegBuffer,"JPEG"),"video JPEG frame");
        const auto avih=aviWords({50000,0,0,0,60,0,1,quint32(videoJpeg.size()),80,48,0,0,0,0});
        const auto strh=QByteArray("vidsMJPG")+aviWords({0,0,0,1,20,0,60,quint32(videoJpeg.size()),0xffffffff,0,0,(48u<<16)|80u});
        const auto strf=aviWords({40,80,48,(24u<<16)|1u})+QByteArray("MJPG")+aviWords({quint32(videoJpeg.size()),0,0,0,0});
        const auto aviHeader=aviChunk("LIST",QByteArray("hdrl")+aviChunk("avih",avih)+aviChunk("LIST",QByteArray("strl")+aviChunk("strh",strh)+aviChunk("strf",strf)));
        QByteArray aviFrames("movi");for(int i=0;i<60;++i)aviFrames+=aviChunk("00dc",videoJpeg);
        const auto avi=aviChunk("RIFF",QByteArray("AVI ")+aviHeader+aviChunk("LIST",aviFrames));
        const auto videoPath=copy.filePath("motion.avi");QFile videoFile(videoPath);require(videoFile.open(QIODevice::WriteOnly) && videoFile.write(avi)==avi.size(),"synthetic video written");videoFile.close();
        addAttachment("video",videoPath,"video/x-msvideo");
        const auto secondVideoPath=copy.filePath("second motion.avi");require(QFile::copy(videoPath,secondVideoPath),"second synthetic video");
        addAttachment("video2",secondVideoPath,"video/x-msvideo");
        {
            // A card first laid out below the reader loads once a change above it
            // brings it on screen, with no scrolling: here the long message folds.
            const auto photo=QVariantMap{{"id","photo"},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text","Image and binary attachment."},{"sent",false},{"attachments",QVariantList{h.attachments["image"].toVariantMap(),h.attachments["binary"].toVariantMap()}}};
            const auto above=QVariantMap{{"id","above"},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"sent_at","2026-09-13T10:00:00Z"},{"text",QString("A line of text above the attachments.").append(QChar('\n')).repeated(160)},{"sent",false}};
            const auto key=mail->property("expansionKey").toString();
            mail->setProperty("expandedMembers",QVariantMap{{key+"above",true},{key+"photo",false}});
            h.backend.m_conversation={{"id","a"},{"subject","Attachment preview"},{"revision",3},{"messages",QVariantList{above,photo}}}; emit h.backend.conversationChanged();
            mail->setProperty("expanded",true);
            auto *reader=find(window->contentItem(),"reader"); require(reader,"reader"); reader->setProperty("contentY",0);
            layout.restart();while(layout.elapsed()<250)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            // Opened under the long message, its cards lay out below the reader.
            QMetaObject::invokeMethod(mail,"toggleMember",Q_ARG(QVariant,QString("photo")),Q_ARG(QVariant,1));
            layout.restart();while(layout.elapsed()<250)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            auto *card=find(window->contentItem(),"attachment_image");
            require(card && card->isVisible() && card->mapToItem(reader,QPointF(0,0)).y()>=reader->height() && card->property("previewUrl").toString().isEmpty(),"a card below the reader loads nothing yet");
            require(reader->property("contentY").toDouble()==0,"the reader has not moved");
            QMetaObject::invokeMethod(mail,"toggleMember",Q_ARG(QVariant,QString("above")),Q_ARG(QVariant,0));
            await([&]{return !card->property("previewUrl").toString().isEmpty();},"the card loads once the fold above brings it on screen");
            require(reader->property("contentY").toDouble()==0,"nothing scrolled the reader");
            mail->setProperty("expandedMembers",QVariantMap{});
        }
        {
            // A video card shows its first frame as a poster, and keeps it once
            // the decoder is released.
            const auto clip=QVariantMap{{"id","clip"},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text","A clip."},{"sent",false},{"attachments",QVariantList{h.attachments["video"].toVariantMap()}}};
            h.backend.m_conversation={{"id","a"},{"subject","Clip"},{"revision",3},{"messages",QVariantList{clip}}}; emit h.backend.conversationChanged();
            QQuickItem *tile=nullptr;
            await([&]{tile=find(window->contentItem(),"attachment_video");return tile && tile->property("videoDuration").toInt()>0;},"the video card reads its length");
            await([&]{return !tile->property("posterUrl").toString().isEmpty() && !find(tile,"attachmentPoster_video");},"the first frame is kept and the decoder released");
            layout.restart();while(layout.elapsed()<200)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            auto *preview=find(tile,"attachmentPreview_video"); require(preview,"the card's preview");
            const auto frame=window->grabWindow();
            const auto at=preview->mapToScene(QPointF(preview->width()*.18,preview->height()*.22))*window->effectiveDevicePixelRatio();
            const QColor painted=frame.pixelColor(at.toPoint()), expected("#94659e");
            if (qAbs(painted.red()-expected.red())>40 || qAbs(painted.green()-expected.green())>40 || qAbs(painted.blue()-expected.blue())>40) qWarning()<<"poster pixel"<<painted.name();
            require(qAbs(painted.red()-expected.red())<=40 && qAbs(painted.green()-expected.green())<=40 && qAbs(painted.blue()-expected.blue())<=40,"the poster shows the video's first frame");
        }
        h.backend.m_conversation={{"id","a"},{"subject","Attachment preview"},{"revision",3},{"messages",QVariantList{QVariantMap{{"id","photo"},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text","Image and binary attachment."},{"sent",false},{"attachments",QVariantList{h.attachments["image"].toVariantMap(),h.attachments["binary"].toVariantMap()}}}}}};
        emit h.backend.conversationChanged();
        await([&]{auto item=find(window->contentItem(),"attachmentPreview_image");return item && item->isEnabled();},"retained image renders in thumbnail");
        auto tile=find(window->contentItem(),"attachment_image");
        await([&]{return tile && tile->property("imageWidth").toInt()==1600;},"decoded image dimensions retained");
        const auto thumb=find(tile,"attachmentThumbnail_image"); require(thumb,"image thumbnail");
        const auto thumbSize=thumb->property("sourceSize").toSize();
        const auto pixelRatio=window->effectiveDevicePixelRatio();
        require(thumbSize.width()>=thumb->width()*pixelRatio && thumbSize.height()>=thumb->height()*pixelRatio,"thumbnail covers the physical display pixels");
        require(thumbSize.width()==qCeil(400*pixelRatio),"thumbnail avoids fixed oversized decode");
        tile->setProperty("maximumWidth",tile->width()-10);
        require(thumb->property("sourceSize").toSize()==thumbSize,"small card resizes reuse the decoded image");
        require(find(window->contentItem(),"attachmentSave_binary")->isEnabled(),"non-image has a save button");
        {
            // The attachments header folds the cards away and back, like a month header.
            auto *caption=find(window->contentItem(),"messageAttachmentsCaption_photo"); require(caption && caption->isVisible(),"attachments header");
            const auto cards=[&]{auto *flow=find(window->contentItem(),"messageAttachments_photo");return flow && flow->isVisible();};
            click(window,caption,Qt::LeftButton); await([&]{return !cards();},"the attachments header folds the cards away");
            click(window,caption,Qt::LeftButton); await(cards,"the attachments header shows them again");
        }
        const auto tileHeight=tile->height();
        for (const auto &feedback: {QString("Saving…"),QString("Saved"),QString("Could not save here. Choose another location and try again.")}) {
            tile->setProperty("feedback",feedback);tile->setProperty("saveFailed",feedback.startsWith("Could"));
            layout.restart();while(layout.elapsed()<40)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(qAbs(tile->height()-tileHeight)<.5,"saving, success and failure retain attachment geometry");
            auto status=find(window->contentItem(),"attachmentStatus_image");
            require(status && status->property("text").toString()==feedback,"feedback is visible inside the reserved attachment row");
        }
        tile->setProperty("feedback",QString{});tile->setProperty("saveFailed",false);
        QMetaObject::invokeMethod(mail,"scrollToMember",Q_ARG(QVariant,QString("photo")));
        layout.restart();while(layout.elapsed()<140)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        const auto cardsCapture=qEnvironmentVariable("FM_ATTACHMENT_CARDS_CAPTURE");
        if(!cardsCapture.isEmpty())require(window->grabWindow().save(cardsCapture),"attachment cards capture");
        click(window,find(window->contentItem(),"attachmentPreview_image"),Qt::LeftButton);
        auto viewer=window->findChild<QObject *>("attachmentViewer");
        await([&]{return viewer && viewer->property("opened").toBool();},"image opens in modal");
        require(mail->property("popupOpen").toBool(),"image modal blocks reader shortcuts");
        await([&]{return viewer->property("ratio").toDouble()==1.6;},"viewer uses the real image aspect ratio");
        require(viewer->property("width").toInt()==window->width() && viewer->property("height").toInt()==window->height(),"media overlay fills viewport");
        const auto viewerImage=viewer->findChild<QQuickItem *>("attachmentViewerImage");require(viewerImage,"viewer image");
        const auto decoded=viewerImage->property("sourceSize").toSize();
        require(decoded.width()>=viewerImage->width()*pixelRatio && decoded.height()>=viewerImage->height()*pixelRatio,"viewer covers physical display pixels");
        require(decoded.width()<viewerImage->width()*pixelRatio+128 && decoded.height()<viewerImage->height()*pixelRatio+128,"viewer decode rounds up by at most one bucket");
        const auto attachmentCapture=qEnvironmentVariable("FM_ATTACHMENT_CAPTURE");
        if(!attachmentCapture.isEmpty()){layout.restart();while(layout.elapsed()<150)QCoreApplication::processEvents(QEventLoop::AllEvents,10);require(window->grabWindow().save(attachmentCapture),"attachment viewer capture");}
        QMetaObject::invokeMethod(viewer,"close");
        const QVariant media=QVariantList{h.attachments["video"].toVariantMap(),h.attachments["image"].toVariantMap()};
        QMetaObject::invokeMethod(viewer,"show",Q_ARG(QVariant,QString("personal")),Q_ARG(QVariant,media),Q_ARG(QVariant,0));
        auto player=viewer->findChild<QObject *>("attachmentMediaPlayer");auto audio=viewer->findChild<QObject *>("attachmentAudio");
        require(player && audio,"actual video player and audio controls exist");
        await([&]{return player->property("duration").toInt()>0 && !viewer->property("preparingPoster").toBool();},"real video decoder loads a paused first frame");
        require(viewer->property("error").toString().isEmpty() && qAbs(viewer->property("ratio").toDouble()-80.0/48)<.01,"decoded video has correct aspect ratio");
        QMetaObject::invokeMethod(viewer,"togglePlayback");await([&]{return player->property("position").toInt()>100;},"video playback advances");
        QMetaObject::invokeMethod(viewer,"togglePlayback");require(player->property("playbackState").toInt()==2,"video pauses");
        player->setProperty("position",1500);require(player->property("position").toInt()>=1500,"video seek updates position");
        audio->setProperty("muted",true);require(audio->property("muted").toBool(),"video audio can mute");
        QMetaObject::invokeMethod(viewer,"step",Q_ARG(QVariant,1));
        await([&]{return !viewer->property("video").toBool() && player->property("source").toUrl().isEmpty();},"next media releases the video source");
        QMetaObject::invokeMethod(viewer,"close");
        {
            // Arrowing from a playing video to the next one plays that one too.
            await([&]{return !viewer->property("visible").toBool();},"the viewer closes");
            const QVariant videos=QVariantList{h.attachments["video"].toVariantMap(),h.attachments["video2"].toVariantMap()};
            QMetaObject::invokeMethod(viewer,"show",Q_ARG(QVariant,QString("personal")),Q_ARG(QVariant,videos),Q_ARG(QVariant,0));
            // The viewer's own player: a closed viewing's one may linger until deleted.
            const auto current=[&]{return viewer->property("player").value<QObject *>();};
            await([&]{auto *first=current();return first && first->property("duration").toInt()>0 && !viewer->property("preparingPoster").toBool();},"the first video loads");
            QMetaObject::invokeMethod(viewer,"togglePlayback");await([&]{return current()->property("position").toInt()>100;},"the first video plays");
            QKeyEvent right(QEvent::KeyPress,Qt::Key_Right,Qt::NoModifier);QCoreApplication::sendEvent(window,&right);
            await([&]{return viewer->property("index").toInt()==1;},"Right moves to the next video");
            await([&]{auto *next=current();return !viewer->property("error").toString().isEmpty() || (next && next->property("source").toUrl().fileName()=="second motion.avi" && next->property("duration").toInt()>0 && !viewer->property("preparingPoster").toBool());},"the next video loads");
            if(!viewer->property("error").toString().isEmpty()) qWarning()<<"viewer error"<<viewer->property("error");
            require(viewer->property("error").toString().isEmpty(),"the next video has no error");
            QMetaObject::invokeMethod(viewer,"togglePlayback");
            await([&]{return current()->property("position").toInt()>100;},"the next video plays");
            QMetaObject::invokeMethod(viewer,"close");
        }
        QHash<QString,QString> saves;
        const auto savedConnection=QObject::connect(&h.backend,&MailBackend::attachmentSaved,[&](const auto &ticket,const auto &error){saves[ticket]=error;});
        const auto destination=copy.filePath("saved.bin");QFile previous(destination);require(previous.open(QIODevice::WriteOnly),"existing target opens");previous.write("previous");previous.close();
        const auto ticket=h.backend.saveAttachment("personal","binary",QUrl::fromLocalFile(destination));
        h.backend.m_account="other";
        await([&]{return saves.contains(ticket);},"attachment save finishes across account navigation");
        require(saves[ticket].isEmpty(),"verified save succeeds");
        require(previous.open(QIODevice::ReadOnly) && previous.readAll()==binaryBytes,"atomic replacement has exact retained bytes");previous.close();
        h.attachments["binary"]["sha256"]=QString(64,'0');
        const auto damaged=h.backend.saveAttachment("personal","binary",QUrl::fromLocalFile(destination));
        await([&]{return saves.contains(damaged);},"bad hash rejects save");
        require(!saves[damaged].isEmpty(),"integrity failure disclosed");
        require(previous.open(QIODevice::ReadOnly) && previous.readAll()==binaryBytes,"failed verification leaves prior destination intact");previous.close();
        const auto remote=h.backend.saveAttachment("personal","binary",QUrl("https://example.test/file"));
        await([&]{return saves.contains(remote);},"remote destination rejected");require(!saves[remote].isEmpty(),"save requires local destination");
        QObject::disconnect(savedConnection);h.backend.m_account="personal";
        // EXIF rotation must affect both dimensions and rendered image orientation.
        const auto orientedPath=copy.filePath("portrait.jpg");QImage camera(40,20,QImage::Format_RGB32);camera.fill(Qt::red);
        require(camera.save(orientedPath,"JPG"),"camera image fixture");
        QFile oriented(orientedPath);require(oriented.open(QIODevice::ReadOnly),"camera JPEG opens");auto jpeg=oriented.readAll();oriented.close();
        const auto exif=QByteArray::fromHex("45786966000049492a0008000000010012010300010000000600000000000000");
        QByteArray segment=QByteArray::fromHex("ffe10022")+exif;jpeg.insert(2,segment);
        require(oriented.open(QIODevice::WriteOnly|QIODevice::Truncate) && oriented.write(jpeg)==jpeg.size(),"EXIF portrait metadata written");oriented.close();
        addAttachment("portrait",orientedPath,"image/jpeg");
        QHash<QString,QVariantMap> previews;
        const auto previewConnection=QObject::connect(&h.backend,&MailBackend::attachmentPreviewReady,[&](const auto &id,const auto &result,bool){previews[id]=result;});
        const auto orientationTicket=h.backend.requestAttachmentPreview("personal","portrait");
        await([&]{return previews.contains(orientationTicket);},"oriented preview returns");
        require(previews[orientationTicket]["width"].toInt()==20 && previews[orientationTicket]["height"].toInt()==40,"EXIF 90-degree rotation transposes dimensions");
        h.attachments["broken-image"]=h.attachments["binary"];h.attachments["broken-image"]["id"]="broken-image";
        const auto invalidImage=h.backend.requestAttachmentPreview("personal","broken-image");
        await([&]{return previews.contains(invalidImage);},"invalid preview returns");
        require(previews[invalidImage]["preview_url"].toString().isEmpty() && !previews[invalidImage]["preview_error"].toString().isEmpty(),"non-image bytes cannot become a preview");
        QObject::disconnect(previewConnection);
        // One shared cursor, viewport paging, edge/range selection and collapsed-group skipping.
        h.backend.m_folder="inbox"; h.backend.m_selected="a"; emit h.backend.queryChanged();
        QMetaObject::invokeMethod(mail,"clearSelection");mail->setProperty("selectionAnchor","a");
        const auto originalRows=h.rows;
        for(int i=0;i<40;++i) {
            auto data=originalRows[0].toMap();data["id"]=QString("page-%1").arg(i,2,10,QChar('0'));data["subject"]=QString("Page navigation %1").arg(i);data["unread"]=false;
            data["group"]=i<20 ? "review" : "held"; h.rows.append(data);
        }
        h.backend.publishConversations(h.rows); emit h.backend.conversationsChanged();
        auto *list=find(window->contentItem(),"conversationList");require(list,"list focus owner");list->forceActiveFocus();
        const auto press=[&](int code,Qt::KeyboardModifiers mods={}) { QKeyEvent key(QEvent::KeyPress,code,mods);QCoreApplication::sendEvent(window,&key);QCoreApplication::processEvents(); };
        press(Qt::Key_PageDown,Qt::ShiftModifier);
        await([&]{return mail->property("selectionRows").toList().size()>3;},"Page Down advances a viewport and extends the range");
        require(mail->property("keyboardNavigation").toBool() && mail->property("pointerRow").toString().isEmpty(),"keyboard owns the single hover cursor");
        require(mail->property("highlightedRow")==mail->property("selectionCursor"),"keyboard highlight follows the range cursor");
        require(list->hasActiveFocus(),"navigation does not leave focus on an old row");
        press(Qt::Key_End,Qt::ShiftModifier);
        await([&]{return mail->property("selectionRows").toList().size()==h.rows.size();},"Shift End selects through the last loaded row");
        press(Qt::Key_PageUp,Qt::ShiftModifier);
        require(mail->property("selectionRows").toList().size()<h.rows.size(),"Page Up returns toward the prior viewport");
        press(Qt::Key_Home,Qt::ShiftModifier);
        require(mail->property("selectionRows").toList().size()==1,"Shift Home returns to the first row");
        QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,"held"),Q_ARG(QVariant,QVariant()));
        QCoreApplication::processEvents(); // The toggle applies just after the click returns.
        press(Qt::Key_End,Qt::ShiftModifier);
        require(mail->property("selectionCursor").toString()=="page-19","End skips collapsed groups");
        QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,"held"),Q_ARG(QVariant,QVariant()));
        QCoreApplication::processEvents();
        QMetaObject::invokeMethod(mail,"clearSelection");h.rows=originalRows;h.backend.publishConversations(h.rows); emit h.backend.conversationsChanged();
        {
            // Spam is listed and opened live through Google operations; nothing asks Core for it.
            const auto pid=h.backend.m_enginePid; h.backend.m_enginePid=QCoreApplication::applicationPid();
            {
                // The Spam and Trash menu closes on a press anywhere else, its
                // button closes it again, and a mailbox change closes it.
                const auto menuShown=[&]{auto item=find(window->contentItem(),"spamScopeButton");return item&&item->isVisible();};
                const auto settle=[&]{QElapsedTimer t;t.start();while(t.elapsed()<120)QCoreApplication::processEvents(QEventLoop::AllEvents,10);};
                click(window,find(window->contentItem(),"binMenuButton"),Qt::LeftButton); await(menuShown,"the menu opens");
                click(window,find(window->contentItem(),"scopeStatCount"),Qt::LeftButton); settle();
                require(!menuShown(),"a press outside closes the Spam and Trash menu");
                click(window,find(window->contentItem(),"binMenuButton"),Qt::LeftButton); await(menuShown,"the menu opens again");
                click(window,find(window->contentItem(),"binMenuButton"),Qt::LeftButton); settle();
                require(!menuShown(),"its button closes it rather than reopening it");
                click(window,find(window->contentItem(),"binMenuButton"),Qt::LeftButton); await(menuShown,"the menu opens once more");
                h.backend.setFolder("sent"); settle();
                require(!menuShown(),"moving to another mailbox closes it");
                h.backend.setFolder("inbox"); settle();
            }
            // Spam sits behind the menu after Drafts; its rows use the normal list, one per thread.
            click(window,find(window->contentItem(),"binMenuButton"),Qt::LeftButton);
            await([&]{auto item=find(window->contentItem(),"spamScopeButton");return item&&item->isVisible();},"the menu after Drafts lists Spam");
            click(window,find(window->contentItem(),"spamScopeButton"),Qt::LeftButton);
            await([&]{return h.backend.folder()=="spam" && h.backend.conversations().size()==1;},"live Spam listing shows one row per thread");
            require(find(window->contentItem(),"conversationList")->isVisible() && h.lastQuery["folder"]!="spam","Spam uses the normal list with no local query");
            const auto spamRow=h.backend.conversations().first().toMap();
            require(spamRow.value("id")=="remote:t9" && spamRow.value("message_count")==2 && spamRow.value("subject")=="You won","a Spam thread counts its messages");
            {
                // Sync refreshes the store every second or two; Spam is live and must not reload or blink.
                const auto lists=h.methods.count("google_spam_list");
                h.backend.loadConversations(false,true); QCoreApplication::processEvents();
                require(h.methods.count("google_spam_list")==lists && h.backend.conversations().size()==1,"a sync refresh neither reloads nor clears Spam");
            }
            require(!find(window->contentItem(),"moreMailFolders"),"the folder menu is gone");
            await([&]{return find(window->contentItem(),"scopeStatLabel")->property("text")=="in spam" && find(window->contentItem(),"scopeStatCount")->property("text").toString()=="1";},"the header counts Spam from Gmail");
            await([&]{return find(window->contentItem(),"binStrip")->isVisible();},"the strip offers to empty Spam");
            {QElapsedTimer settle;settle.start();while(settle.elapsed()<200)QCoreApplication::processEvents(QEventLoop::AllEvents,10);}
            h.backend.openConversation("remote:t9");
            await([&]{auto *bar=find(window->contentItem(),"binActionsRow");return bar && bar->isVisible() && h.backend.conversation().value("messages").toList().size()==2;},"an opened Spam thread reads like any thread, with the Spam actions");
            require(!find(window->contentItem(),"conversationActionsRow")->isVisible(),"Spam has no Reply or Delay");
            // FM_BIN_CAPTURE=<dir> keeps frames of the Spam view for visual checks.
            const auto shots=qEnvironmentVariable("FM_BIN_CAPTURE");
            auto shot=[&](const QString &name){ if(!shots.isEmpty()) require(QDir().mkpath(shots) && window->grabWindow().save(shots+"/"+name+".png"),"Spam frame saved"); };
            shot("spam-thread");
            // Delete permanently asks first; an account without full access is asked to reconnect.
            click(window,find(window->contentItem(),"deleteForeverButton"),Qt::LeftButton);
            await([&]{auto *m=find(window->contentItem(),"binConfirmMessage");return m && m->isVisible();},"deleting for good asks first");
            shot("spam-confirm");
            require(find(window->contentItem(),"binConfirmMessage")->property("text").toString()==QString::fromUtf8("\u201cOld invoice\u201d will be deleted for good. This can\u2019t be undone."),"the confirm names the subject");
            click(window,find(window->contentItem(),"binConfirmAction"),Qt::LeftButton);
            await([&]{auto *m=find(window->contentItem(),"reconnectFullAccessAction");return m && m->isVisible();},"missing full access offers to reconnect");
            click(window,find(window->contentItem(),"reconnectFullAccessCancel"),Qt::LeftButton);
            await([&]{auto *m=find(window->contentItem(),"reconnectFullAccessAction");return !m || !m->isVisible();},"the reconnect prompt closes");
            {auto *button=find(window->contentItem(),"notSpamButton"); require(button && button->isVisible() && button->isEnabled(),"Move to Inbox is offered");}
            click(window,find(window->contentItem(),"notSpamButton"),Qt::LeftButton);
            await([&]{return h.methods.contains("google_not_spam");},"Move to Inbox asks Google");
            await([&]{return h.backend.conversations().isEmpty() && h.backend.actionToast().value("text").toString().startsWith("Moved to Inbox");},"Move to Inbox removes the row and says so");
            require(h.methods.contains("google_not_spam"),"not spam goes to Google");
            await([&]{auto *t=find(window->contentItem(),"binEmptyText");return t && t->isVisible() && t->property("text")=="No spam. Nice.";},"empty Spam says so");
            require(!h.backend.m_views.value("personal").toMap().value("folder").toString().startsWith("spam"),"Spam is not remembered as the view to reopen");
            h.backend.setFolder("inbox"); h.backend.m_enginePid=pid;
            await([&]{return h.backend.folder()=="inbox" && !find(window->contentItem(),"binStrip")->isVisible();},"leaving Spam restores the Inbox list");
        }
        require(!warnings,"no QML errors in production selection/splash controls");
        const auto capture=qEnvironmentVariable("FM_SELECTION_CAPTURE");
        if(!capture.isEmpty()) {
            h.backend.m_folder="inbox"; emit h.backend.queryChanged();
            auto preview=h.rows[0].toMap();preview["summary"]="You granted Example Mail access to your account data. Review access if you did not authorize it.";preview["subject"]="Security alert";h.rows[0]=preview;h.backend.publishConversations(h.rows); emit h.backend.conversationsChanged();
            h.backend.m_selected="a";
            h.backend.m_conversation={{"id","a"},{"subject","Security alert"},{"revision",3},{"messages",QVariantList{QVariantMap{{"id","a-original"},{"subject","Security alert"},{"sender_name","Account security"},{"sender_email","accounts@example.test"},{"recipient","test@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text","You allowed Example Mail access to some of your account data. If you did not authorize this, review your account access."},{"kind","automated"},{"sent",false},{"revision",1},{"categories",QVariantList{}}}}}};
            emit h.backend.conversationChanged();
            await([&]{auto *item=find(window->contentItem(),"messageSummary_a-original");return item && item->isVisible();},"per-message summary painted");
            QElapsedTimer paint;paint.start();while(paint.elapsed()<100)QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(window->grabWindow().save(capture),"native mail summary/list capture");
        }
    }
public:
    static void accountControls() {
        Harness h;
        auto ordinary=h.engine.customRequest;
        int scans=0,starts=0,stops=0,preferenceWrites=0,settingsRevision=0,labelReads=0;QJsonObject savedAccount,savedPreferences,savedAssist; bool stopped=false; QString requestedAccount,requestedRange;
        h.engine.customRequest=[&](auto *peer,const auto &request) {
            const auto method=request["method"].toString();QJsonObject result;
            if(method=="preferences_save") {++preferenceWrites;savedPreferences=request["params"].toObject()["preferences"].toObject();result={{"revision",preferenceWrites},{"preferences",savedPreferences}};}
            else if(method=="preferences_get")result={{"revision",preferenceWrites},{"preferences",QJsonObject::fromVariantMap(h.backend.preferences())}};
            else if(method=="account_update") {savedAccount=request["params"].toObject();auto row=h.backend.m_allAccounts[0].toMap();row["name"]=savedAccount["name"].toString();row["mail_retention"]=savedAccount["mail_retention"].toString();row["metadata_revision"]=1;h.backend.m_allAccounts[0]=row;result=QJsonObject::fromVariantMap(row);}
            else if(method=="assist_backlog") {++scans;result={{"account","personal"},{"enabled",true},{"ranges",QJsonObject{{"week",7},{"month",40},{"year",500},{"all",600}}}};}
            else if(method=="assist_progress") {
                const bool batch=starts>0&&!stopped;
                result={{"accounts",QJsonArray{QJsonObject{{"account","personal"},{"local",1200},{"eligible",1000},{"done",400},{"queued",batch?7:0},{"unprocessed",batch?593:600},
                    {"model_configured",true},{"enabled",true},{"summaries",true},{"classification",true},{"paused",false},{"seconds_per_message",3.0},{"catching_up",batch}}}}};
            }
            else if(method=="assist_settings_get") result={{"revision",settingsRevision},{"settings",QJsonObject{{"accounts",QJsonObject{}}}}};
            else if(method=="assist_settings_save") {++settingsRevision;savedAssist=request["params"].toObject();result={{"revision",settingsRevision},{"settings",savedAssist["settings"]}};}
            else if(method=="assist_jobs")result={{"items",QJsonArray{}}};
            else if(method=="assist_catch_up") {++starts;requestedAccount=request["account"].toString();requestedRange=request["params"].toObject()["range"].toString();result={{"accounts",QJsonArray{}}};}
            else if(method=="assist_cancel") {++stops;stopped=true;result={{"status","canceled"}};}
            else if(method=="label_catalog") {++labelReads;result={{"categories",QJsonArray{QJsonObject{{"id","receipts"},{"name","Receipts"},{"conversation_count",2}}}}};}
            else return ordinary(peer,request);
            peer->write(reply(request["id"].toString(),result));return true;
        };
        QTemporaryDir copy;const auto path=copy.filePath("AccountsTest");require(QDir().mkpath(path),"account QML module");
        QByteArray module("module AccountsTest\nsingleton Theme 1.0 Theme.qml\n");
        for(const auto &name:QDir(QString(FM_GUI_SOURCE_DIR)).entryList({"*.qml"},QDir::Files)) {
            require(QFile::copy(QString(FM_GUI_SOURCE_DIR)+'/'+name,path+'/'+name),"production account QML");
            if(name=="ShortcutRegistry.qml") module+="singleton ";
            if(name!="Theme.qml") module+=name.chopped(4).toUtf8()+" 1.0 "+name.toUtf8()+'\n';
        }
        QFile registration(path+"/qmldir");require(registration.open(QIODevice::WriteOnly),"account QML registration");registration.write(module);registration.close();
        QQmlApplicationEngine qml; FmIconItem::registerType();bool warnings=false;
        QObject::connect(&qml,&QQmlEngine::warnings,[&](const auto &errors){warnings=true;for(const auto &error:errors)qWarning()<<error.toString();});
        qml.addImportPath(copy.path());qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport AccountsTest\nApplicationWindow {visible:true;width:1280;height:800;color:Theme.canvas;palette.text:Theme.text;palette.base:Theme.surface;palette.window:Theme.canvas;palette.buttonText:Theme.text;function appearance(theme,size){Theme.themeId=theme;Theme.textSize=size} SettingsView {anchors.fill:parent;page:'Accounts'} }",QUrl::fromLocalFile(copy.filePath("Test.qml")));
        require(!qml.rootObjects().isEmpty(),"native Accounts loads");auto *window=qobject_cast<QQuickWindow*>(qml.rootObjects().first());
        auto *settings=find(window->contentItem(),"settingsView");require(settings,"account settings present");
        // The account is syncing, so its local model stage runs rather than pauses.
        h.backend.m_googleSync={{"personal",QVariantMap{{"account","personal"},{"active",true},{"owned",true}}}};emit h.backend.googleSyncChanged();
        await([&]{auto *button=find(window->contentItem(),"accountBacklog_personal");return button&&button->isVisible()&&button->isEnabled();},"unprocessed mail offers Catch up");
        require(scans==0 && starts==0,"viewing Accounts reads cheap progress: no mail scan and no inference");
        {
            // Sync refreshing the account's counts updates its card in place: the
            // same card, its labels shown throughout and not read again.
            await([&]{auto *labels=find(window->contentItem(),"accountLabels_personal");return labels && labels->property("text")=="Receipts";},"the card shows the account's labels");
            QPointer<QQuickItem> card=find(window->contentItem(),"accountCard_personal"); require(card,"the account card");
            const auto reads=labelReads;
            for (int n=0; n<5; ++n) {
                auto row=h.backend.m_allAccounts[0].toMap(); row["conversation_count"]=row["conversation_count"].toInt()+1;
                h.backend.m_allAccounts={row}; h.backend.publishAccounts(h.backend.m_allAccounts); emit h.backend.accountsChanged();
                QCoreApplication::processEvents();
                require(card && find(window->contentItem(),"accountCard_personal")==card.data(),"the card is kept, not rebuilt");
                require(find(card,"accountLabels_personal")->property("text")=="Receipts","its labels stay on screen");
            }
            QElapsedTimer settled; settled.start(); while(settled.elapsed()<100) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(labelReads==reads,"the labels are not read again");
        }
        require(find(window->contentItem(),"accountAnalysisCount_personal")->property("text")=="400 of 1k","processed of eligible mail displayed");
        require(find(window->contentItem(),"accountAnalysisStatus_personal")->property("text")=="600 not processed","unprocessed mail is named on the model row");
        {auto *status=find(window->contentItem(),"accountAnalysisStatus_personal");require(status->width()+0.5>=status->property("implicitWidth").toReal() && !status->property("truncated").toBool(),"a meter's status shows in full, not elided");}
        {
            // While Google's list is counted the mail row says how many so
            // far over a shimmering bar; once counted it has a real total.
            const auto original=h.backend.m_allAccounts;
            auto setSync=[&](const QVariantMap &sync){auto row=original[0].toMap();auto google=row["google"].toMap();google["sync"]=sync;row["google"]=google;h.backend.m_allAccounts={row};h.backend.publishAccounts(h.backend.m_allAccounts);emit h.backend.accountsChanged();};
            auto text=[&](const QString &name){auto *item=find(window->contentItem(),name);return item?item->property("text").toString():QString();};
            setSync({{"inventory_complete",false},{"retained_sources",13204},{"observed_messages",13204},{"inventory_counting",8512}});
            await([&]{return text("accountMailCount_personal")=="13.2k" && text("accountMailStatus_personal")==QString::fromUtf8("Counting · 8.5k found");},"counting shows what is local and how many Google has so far");
            setSync({{"inventory_complete",false},{"retained_sources",13204},{"observed_messages",13204},{"inventory_total",21388}});
            await([&]{return text("accountMailCount_personal")=="13.2k of 21.4k" && text("accountMailStatus_personal").startsWith("61%");},"a counted total gives the real share");
            {
                // The top bar's account underline reads the same view: the same share.
                QVariant view;QMetaObject::invokeMethod(qvariant_cast<QObject*>(settings->property("progress")),"view",Q_RETURN_ARG(QVariant,view),Q_ARG(QVariant,QString("personal")),Q_ARG(QVariant,false));
                require(view.toMap()["busy"].toBool() && qAbs(view.toMap()["progress"].toDouble()-13204./21388)<0.01,"the account tab's underline shows the download's real share");
                QVariant whole;QMetaObject::invokeMethod(qvariant_cast<QObject*>(settings->property("progress")),"view",Q_RETURN_ARG(QVariant,whole),Q_ARG(QVariant,QString("personal")),Q_ARG(QVariant,true));
                require(whole.toMap()["mail"].toMap()["count"].toString()==QLocale().toString(13204.,'f',0)+" of "+QLocale().toString(21388.,'f',0),"the hover card counts in whole numbers");
            }
            {
                // Rechecking kept mail after Gmail history expires moves the bar
                // and says so, rather than a download that stands still.
                setSync({{"inventory_complete",false},{"retained_sources",1000},{"observed_messages",1000},{"inventory_total",5000},{"unchecked_messages",250}});
                await([&]{return text("accountMailStatus_personal")=="Checking 75%";},"a recheck of kept mail shows its own progress");
                QVariant view;QMetaObject::invokeMethod(qvariant_cast<QObject*>(settings->property("progress")),"view",Q_RETURN_ARG(QVariant,view),Q_ARG(QVariant,QString("personal")),Q_ARG(QVariant,false));
                const auto mail=view.toMap()["mail"].toMap();
                require(mail["status"].toString()==QString::fromUtf8("Checking downloaded mail · 250 to go"),"the hover card names the recheck");
                require(qAbs(view.toMap()["progress"].toDouble()-.75)<0.01,"the account tab's underline follows the recheck");
            }
            setSync({{"inventory_complete",false},{"retained_sources",21400},{"observed_messages",21400},{"inventory_total",21388}});
            await([&]{return text("accountMailStatus_personal").startsWith("99%");},"mail arriving during the download never shows 100% early");
            setSync({{"inventory_complete",true},{"retained_sources",21400},{"observed_messages",21400},{"inventory_total",21388}});
            await([&]{return text("accountMailStatus_personal")=="Complete";},"a finished inventory is complete");
            // A message Gmail cannot deliver is settled: no bar stuck at 99%, and
            // the status stays plain; only the count's tooltip names it.
            setSync({{"inventory_complete",true},{"retained_sources",51618},{"observed_messages",51620},{"undownloadable_messages",2}});
            await([&]{return text("accountMailStatus_personal")=="Complete";},"messages that cannot download do not hold the download open");
            {
                QVariant settled;QMetaObject::invokeMethod(qvariant_cast<QObject*>(settings->property("progress")),"view",Q_RETURN_ARG(QVariant,settled),Q_ARG(QVariant,QString("personal")),Q_ARG(QVariant,false));
                const auto mail=settled.toMap()["mail"].toMap();
                require(mail["status"].toString()=="Download complete","the hover card does not list messages that cannot download");
                require(mail["countTitle"].toString().contains("2 messages can't be downloaded"),"the count's tooltip still says which messages are missing");
            }
            h.backend.m_allAccounts=original;h.backend.publishAccounts(original);emit h.backend.accountsChanged();
        }
        const auto captureDir=qEnvironmentVariable("FM_BACKLOG_CAPTURE_DIR");
        auto capture=[&](const QString &name) {
            bool painted=false;const auto connection=QObject::connect(window,&QQuickWindow::frameSwapped,[&]{painted=true;});window->requestUpdate();await([&]{return painted;},"account frame");QObject::disconnect(connection);
            if(!captureDir.isEmpty()){require(QDir().mkpath(captureDir),"account capture directory");require(window->grabWindow().save(captureDir+"/"+name+".png"),"account frame saved");}
        };
        for(const auto theme:{"graphite","paper","terminal"}) {QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,"default"));capture(QString("accounts-")+theme);}
        click(window,find(window->contentItem(),"accountBacklog_personal"),Qt::LeftButton);
        await([&]{auto *item=find(window->contentItem(),"backlogRange_personal_week");return item&&item->isVisible()&&item->isEnabled();},"Catch up counts each range when it opens");
        require(scans==1,"only opening Catch up scans the account's mail");
        capture("backlog-menu");click(window,find(window->contentItem(),"backlogRange_personal_week"),Qt::LeftButton);
        await([&]{return starts==1;},"range starts one real account command");
        require(requestedAccount=="personal"&&requestedRange=="week","Catch up widens exactly the selected account and range");
        await([&]{auto *status=find(window->contentItem(),"accountAnalysisStatus_personal");return status&&status->property("text").toString().startsWith("40% · 7 queued");},"Catch up shows as queued work with its estimate");
        settings->setProperty("page","General");settings->setProperty("page","Accounts");
        capture("backlog-running");
        {
            auto *badge=find(window->contentItem(),"accountRetention_personal");
            require(badge && badge->isVisible(),"the account shows its retention badge");
            bool everything=false;
            for (auto child : badge->childItems()) everything |= child->property("text")=="Keeps everything";
            require(everything,"an account without a limit keeps everything");
        }
        auto *pause=find(window->contentItem(),"accountAiPause_personal");require(pause&&pause->isVisible(),"queued work can be paused");
        click(window,pause,Qt::LeftButton);
        await([&]{return savedAssist["settings"].toObject()["accounts"].toObject()["personal"].toObject()["paused"].toBool();},"pausing the model saves the account's pause");
        require(stops==0,"pausing holds the queue; Catch up is not cancelled");
        // The avatar opens the colour picker; a swatch saves through account_update.
        click(window,find(window->contentItem(),"accountAvatar_personal"),Qt::LeftButton);
        await([&]{auto *swatch=find(window->contentItem(),"accountColor_6FB5B2");return swatch&&swatch->isVisible();},"avatar opens the 16-swatch colour picker");
        capture("color-picker");
        click(window,find(window->contentItem(),"accountColor_6FB5B2"),Qt::LeftButton);
        await([&]{return savedAccount["color"].toString()=="#6FB5B2";},"a swatch saves the account colour");
        savedAccount={};
        // Edit › on the Labels row opens the per-account labels modal.
        click(window,find(window->contentItem(),"editLabels_personal"),Qt::LeftButton);
        await([&]{auto *title=find(window->contentItem(),"labelsDialogTitle");return title&&title->isVisible();},"Labels opens as a modal from Accounts");
        require(find(window->contentItem(),"labelsDialogTitle")->property("text").toString().startsWith("Labels · "),"the modal names its account");
        capture("labels-dialog");
        click(window,find(window->contentItem(),"closeLabelsDialog"),Qt::LeftButton);
        await([&]{auto *title=find(window->contentItem(),"labelsDialogTitle");return !title||!title->isVisible();},"the labels modal closes");
        // The Signature row opens its editor; Save needs an edit and saves through account_update.
        click(window,find(window->contentItem(),"editSignature_personal"),Qt::LeftButton);
        await([&]{auto *editor=find(window->contentItem(),"signature_personal");return editor&&editor->isVisible();},"Signature opens its editor from Accounts");
        require(!find(window->contentItem(),"saveSignature_personal")->isEnabled(),"Save waits for an edit");
        find(window->contentItem(),"signature_personal")->setProperty("text","Alex\nWindscribe  \n\n");
        capture("signature-dialog");
        click(window,find(window->contentItem(),"saveSignature_personal"),Qt::LeftButton);
        await([&]{return savedAccount["signature_text"].toString()=="Alex\nWindscribe";},"the signature saves without trailing blank lines");
        savedAccount={};
        window->resize(960,720);QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,"paper"),Q_ARG(QVariant,"large"));capture("accounts-narrow-large");
        auto *button=find(window->contentItem(),"accountBacklog_personal");require(button->mapToScene({button->width(),0}).x()<=window->width(),"backlog control stays within narrow settings");
        window->resize(1280,900);QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,"graphite"),Q_ARG(QVariant,"default"));
        auto editor=window->findChild<QObject *>("accountEditor");require(editor,"real account editor exists");
        require(QMetaObject::invokeMethod(editor,"show",Q_ARG(QVariant,h.backend.m_allAccounts[0])),"open account editor");
        await([&]{return editor->property("opened").toBool();},"account editor opens");
        capture("account-editor");
        find(window->contentItem(),"editAccountName")->setProperty("text","Renamed account");
        find(window->contentItem(),"editAccountRetention")->setProperty("currentIndex",2);
        find(window->contentItem(),"editAccountMailMode")->setProperty("currentIndex",0);
        QMetaObject::invokeMethod(find(window->contentItem(),"saveAccountEdit"),"clicked");
        await([&]{return !savedAccount.isEmpty();},"account edit reaches shared command");
        require(savedAccount["name"]=="Renamed account" && savedAccount["mail_retention"]=="2_years" && savedAccount["sync"].toObject()["mail_access"]=="read","name, retention and access saved together");
        await([&]{return !editor->property("opened").toBool();},"saved editor closes");
        // Missing Calendar grant opens reconnect; close it before shortcut recording.
        if(auto dialog=window->findChild<QObject *>("googleSignInDialog"))QMetaObject::invokeMethod(dialog,"close");
        settings->setProperty("page","Shortcuts");
        auto shortcut=find(window->contentItem(),"shortcut_reply");require(shortcut,"custom shortcut row exists");
        auto press=[&](int key,Qt::KeyboardModifiers mods=Qt::NoModifier){QKeyEvent down(QEvent::KeyPress,key,mods),up(QEvent::KeyRelease,key,mods);QCoreApplication::sendEvent(window,&down);QCoreApplication::sendEvent(window,&up);};
        QMetaObject::invokeMethod(shortcut,"clicked");press(Qt::Key_D);
        await([&]{return preferenceWrites>0 && !h.backend.preferencesPending();},"recorded shortcut saves immediately");
        require(savedPreferences["shortcuts"].toObject()["reply"]=="D" && savedPreferences["shortcuts"].toObject()["delay"].isNull(),"same-scope conflict moves key and clears previous action");
        QMetaObject::invokeMethod(shortcut,"clicked");const auto previousWrites=preferenceWrites;press(Qt::Key_K,Qt::ControlModifier);
        require(settings->property("recordingShortcut").toBool() && preferenceWrites==previousWrites,"fixed search chord cannot be reassigned");
        press(Qt::Key_Escape);require(!settings->property("recordingShortcut").toBool(),"Escape cancels recording");
        capture("shortcuts-edited");
        require(!warnings,"Accounts coverage/menu/progress has no QML errors");
    }
    // Remote search rows join the list by date, open live, and change nothing
    // here; unsubscribing goes to the engine by method and is remembered.
    static void remoteMail() {
        Harness h;
        h.backend.m_openSections << "done:2020-01";
        // Google operations open their own connection to the fake engine.
        h.backend.m_enginePid=QCoreApplication::applicationPid();
        h.backend.loadConversations(); await([&]{return !h.backend.m_loading;},"mailbox loads before a remote search");
        h.backend.setQuery("invoice"); await([&]{return !h.backend.m_loading && h.lastQuery["query"]=="invoice";},"local search runs");
        require(h.remoteSearches.isEmpty(),"the server is not searched while the switch is off");
        h.backend.setRemoteSearch(true);
        await([&]{return h.backend.remoteSearchState().value("count").toInt()==1;},"remote search answers");
        require(h.remoteSearches.size()==1 && h.remoteSearches[0]["query"]=="invoice" && h.remoteSearches[0]["folder"]=="inbox","the server search names the query and folder");
        auto rows=h.backend.conversations();
        require(rows.size()==4 && rows.last().toMap().value("id")=="remote:t1" && rows.last().toMap().value("section")=="done:2020-01","an older remote row joins Done by month after local rows");
        require(h.backend.snapshotMailRows({"remote:t1"}).isEmpty(),"remote rows are left out of bulk actions");
        const auto before=h.methods.size();
        h.backend.openConversation("remote:t1");
        await([&]{return h.backend.conversation().value("remote").toBool() && h.backend.conversation().value("messages").toList().size()==2;},"a remote result opens its whole thread");
        const auto latest=h.backend.conversation().value("messages").toList().last().toMap();
        require(!latest.value("reader_sections").toList().isEmpty() && !latest.contains("body_html"),"the newest remote message is formatted like stored mail");
        const auto opened=h.methods.mid(before);
        if (opened.contains("draft_get") || opened.contains("flag")) qInfo()<<opened;
        require(!opened.contains("draft_get") && !opened.contains("flag"),"opening a remote thread reads no draft and marks nothing read");
        h.backend.setStarred(true); QCoreApplication::processEvents();
        require(h.writes.isEmpty(),"a remote thread cannot be changed here");
        require(h.backend.m_views.value("personal").toMap().value("selected").toString().isEmpty() || !h.backend.m_views.value("personal").toMap().value("selected").toString().startsWith("remote:"),"a remote thread is not reopened after restart");
        h.backend.setQuery("other");
        require(!h.backend.conversations().last().toMap().value("remote").toBool(),"remote rows for an old query leave at once");
        await([&]{return h.remoteSearches.size()==2;},"a new query searches the server again");
        h.backend.setRemoteSearch(false);
        require(!h.backend.conversations().last().toMap().value("remote").toBool(),"turning the switch off removes remote rows");
        // Unsubscribe by the method the reader read, then remember it.
        h.backend.setQuery(""); await([&]{return !h.backend.m_loading;},"search clears");
        h.backend.openConversation("a");
        await([&]{return h.backend.conversation().value("id")=="a" && h.backend.conversation().contains("mailing_list");},"a list conversation carries its mailing-list counts");
        bool replied=false, accepted=false;
        QObject::connect(&h.backend,&MailBackend::unsubscribeReady,[&](auto,auto,bool ok){replied=true;accepted=ok;});
        h.backend.unsubscribe({{"id","a-original"},{"source_version",QString(64,char(97))},{"method","one_click"}});
        await([&]{return replied;},"one-click unsubscribe answers");
        require(accepted && h.methods.contains("message_unsubscribe") && !h.methods.contains("google_unsubscribe_mail"),"one-click goes to the engine's background POST");
        require(h.backend.conversation().value("mailing_list").toMap().value("unsubscribed").toMap().value("method")=="one_click","the reader remembers the unsubscribe");
        replied=false;
        h.backend.unsubscribe({{"id","a-original"},{"source_version",QString(64,char(97))},{"method","mailto"}});
        await([&]{return replied;},"mailto unsubscribe answers");
        require(h.methods.contains("google_unsubscribe_mail"),"a mailto list is unsubscribed by email through Google");
    }
    // Years of Done mail with most months folded: a clicked month header stays
    // put while it opens and closes, however late sync lets the refresh land.
    static void monthToggles() {
        Harness h;
        h.filterCollapsed = true;
        const auto make=[](const QString &id,const QString &group,bool unread,const QString &at){
            return QVariant(QVariantMap{{"id",id},{"subject","Month "+id},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"latest_at",at},
                {"message_ids",QVariantList{id+"-original"}},{"revision",3},{"message_count",1},{"categories",QVariantList{}},
                {"group",group},{"kind","people"},{"unread",unread},{"starred",false}}); };
        h.rows.clear();
        for (int n=0; n<8; ++n) h.rows.append(make(QString("r%1").arg(n),"review",true,"2026-09-14T12:00:00Z"));
        QJsonArray months; QStringList monthKeys;
        for (int m=0; m<36; ++m) {
            const auto date=QDate(2026,8,15).addMonths(-m);
            const auto month=date.toString("yyyy-MM");
            months.append(QJsonObject{{"month",month},{"count",5}}); monthKeys.append("done:"+month);
            for (int n=0; n<5; ++n) h.rows.append(make(QString("d%1-%2").arg(m).arg(n),"done",false,date.toString("yyyy-MM-dd")+"T12:00:00Z"));
        }
        h.summary={{"filtered_group_counts",QJsonObject{{"review",8},{"held",0}}},{"done_months",months}};
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Months.qml")));
        require(!qml.rootObjects().isEmpty(),"month MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"month window exposed");
        const auto settle=[&](int ms=150){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,10); };
        // Opened once, every month's rows load.
        h.backend.m_openSections=monthKeys;
        h.backend.m_folder="inbox"; h.backend.m_selected.clear(); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==h.rows.size();},"years of Inbox mail load");
        // Every month but the newest two is folded.
        h.backend.setSections({},monthKeys.mid(0,2));
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==8+10;},"older months fold");
        settle();
        auto *list=find(window->contentItem(),"conversationList"); require(list,"month list");
        // To review's band stays at the top while its rows scroll, and only then.
        auto *sticky=find(window->contentItem(),"mailStickyGroup"); require(sticky,"sticky band");
        require(!sticky->isVisible(),"no sticky band while the real header shows");
        if (qEnvironmentVariableIsSet("FM_MONTH_CAPTURE")) window->grabWindow().save(qEnvironmentVariable("FM_MONTH_CAPTURE")+"/months.png");
        list->setProperty("contentY",list->property("originY").toReal()+120); settle();
        require(sticky->isVisible() && sticky->property("stuck")=="review" && std::abs(sticky->mapToScene({0,0}).y()-list->mapToScene({0,0}).y())<1,"To review's band holds the top while its rows scroll");
        if (qEnvironmentVariableIsSet("FM_MONTH_CAPTURE")) window->grabWindow().save(qEnvironmentVariable("FM_MONTH_CAPTURE")+"/sticky.png");
        list->setProperty("contentY",list->property("originY")); settle();
        require(!sticky->isVisible(),"back at the top the band lets go");
        QMetaObject::invokeMethod(list,"positionViewAtEnd"); settle();
        // A folded month in the middle of the screen.
        QQuickItem *header=nullptr; QString group;
        for (const auto &key : monthKeys.mid(2)) {
            auto *item=find(window->contentItem(),"mailGroup_"+key);
            if (!item || !item->isVisible()) continue;
            const auto y=item->mapToScene({0,0}).y();
            if (y>250 && y<550) { header=item; group=key; break; }
        }
        require(header,"a folded month header is on screen");
        // Retired delegates can linger hidden; only the painted header counts.
        const auto painted=[&]()->QQuickItem* { QQuickItem *found=nullptr; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(found) return; if(item->objectName()=="mailGroup_"+group && item->isVisible() && item->height()>0) { found=item; return; } for(auto *c:item->childItems()) walk(c); }; walk(window->contentItem()); return found; };
        const auto sceneY=[&]{ auto *item=painted(); return item ? item->mapToScene({0,0}).y() : -1e9; };
        const auto before=sceneY();
        const auto steady=[&](const char *message){ const auto now=sceneY(); if (std::abs(now-before)>1) { QList<qreal> ys; std::function<void(QQuickItem*)> all=[&](QQuickItem *item){ if(!item->isVisible() || item->opacity()==0) return; if(item->objectName()=="mailGroup_"+group && item->height()>0) ys<<item->mapToScene({0,0}).y(); for(auto *c:item->childItems()) all(c); }; all(window->contentItem()); qWarning()<<"header moved from"<<before<<"to"<<now<<"; painted copies at"<<ys; require(false,message); } };
        h.holdLists=true;
        click(window,header,Qt::LeftButton); settle(50);
        steady("the month header stays put as it opens");
        require(shownRows(h.backend)==8+15,"the month's rows show at once from memory");
        // Sync keeps the refresh waiting past the old two-second hold.
        QElapsedTimer waited; waited.start();
        while (waited.elapsed()<2600) { QCoreApplication::processEvents(QEventLoop::AllEvents,10); steady("the header holds while the refresh waits"); }
        h.releaseLists(); settle(300);
        steady("the late refresh lands without moving the header");
        // Quick repeated toggles, each refresh arriving late and out of step.
        for (int n=0; n<6; ++n) {
            auto *item=painted(); require(item,"the month header remains");
            click(window,item,Qt::LeftButton); settle(40); steady("repeated toggles keep the header in place");
        }
        settle(2200); steady("the header holds past two seconds of pending refreshes");
        h.releaseLists(); settle(300);
        steady("the settled list keeps the header where it was clicked");
        h.holdLists=false;
    }
    // A search opens every month that has matches; its own folds last until
    // the search changes, and clearing it restores the mailbox's folds.
    static void searchOpensMonths() {
        Harness h;
        h.filterCollapsed = true;
        const auto make=[](const QString &id,const QString &at){
            return QVariant(QVariantMap{{"id",id},{"subject","Match "+id},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"latest_at",at},
                {"message_ids",QVariantList{id+"-original"}},{"revision",3},{"message_count",1},{"categories",QVariantList{}},
                {"group","done"},{"kind","people"},{"unread",false},{"starred",false}}); };
        h.rows.clear();
        QJsonArray months; QStringList monthKeys;
        for (int m=0; m<6; ++m) {
            const auto date=QDate(2026,8,15).addMonths(-m);
            months.append(QJsonObject{{"month",date.toString("yyyy-MM")},{"count",2}}); monthKeys.append("done:"+date.toString("yyyy-MM"));
            for (int n=0; n<2; ++n) h.rows.append(make(QString("s%1-%2").arg(m).arg(n),date.toString("yyyy-MM-dd")+"T12:00:00Z"));
        }
        h.summary={{"filtered_group_counts",QJsonObject{{"review",0},{"held",0}}},{"done_months",months}};
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Search.qml")));
        require(!qml.rootObjects().isEmpty(),"search MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"search window exposed");
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"mail view");
        const auto collapsed=[&](const QString &key){ QVariant result; QMetaObject::invokeMethod(mail,"groupCollapsed",Q_RETURN_ARG(QVariant,result),Q_ARG(QVariant,key)); return result.toBool(); };
        h.backend.m_folder="inbox"; h.backend.m_openSections=monthKeys.mid(0,1); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==2;},"only the newest month is open");
        require(collapsed(monthKeys[3]),"older months are folded in the mailbox");
        const auto savedView=h.backend.m_views.value("personal").toMap();
        // Typed a letter at a time; the search runs from the app's own timer.
        for (const auto *typed : {"M","Ma","Mat","Matc","Match"}) { h.backend.setQuery(typed); QElapsedTimer t; t.start(); while (t.elapsed()<40) QCoreApplication::processEvents(QEventLoop::AllEvents,5); }
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==12;},"a search lists every month's matches");
        for (const auto &key : monthKeys) require(!collapsed(key),"every month with matches is open while searching");
        h.backend.setSectionCollapsed(monthKeys[2],true);
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==10;},"a month folded during the search");
        const auto viewNow=h.backend.m_views.value("personal").toMap();
        require(collapsed(monthKeys[2]) && viewNow.value("openSections")==savedView.value("openSections") && viewNow.value("collapsedSections")==savedView.value("collapsedSections"),
            "a search's fold is not saved with the mailbox");
        // Cleared as the search box's × does.
        h.backend.setProperty("query", QString());
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==2;},"clearing the search restores the mailbox's folds");
        QElapsedTimer later; later.start(); while (later.elapsed()<600) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        require(shownRows(h.backend)==2 && collapsed(monthKeys[3]) && !collapsed(monthKeys[0]),"the mailbox folds are back");
        for (int m=1; m<6; ++m) { auto *header=find(window->contentItem(),"mailGroup_"+monthKeys[m]); require(collapsed(monthKeys[m]),"every older month folds again"); Q_UNUSED(header); }
        qInfo()<<"A search opens every month with matches and leaves the mailbox's folds alone.";
    }
    // The Inbox list is exact however it changes. Every item is one row high,
    // so its length (and the scrollbar) is what it holds plus what is still to
    // load. Scrolling, paging, folding and opening months never change it by
    // more than what they fold, never blank the view and never paint a header
    // twice; a kept row stays in its own month.
    static void listGeometry() {
        Harness h;
        h.filterCollapsed = true; h.pageSize = 40;
        const auto make=[](const QString &id,const QString &group,bool unread,const QString &at){
            return QVariant(QVariantMap{{"id",id},{"subject","Geometry "+id},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"latest_at",at},
                {"message_ids",QVariantList{id+"-original"}},{"revision",3},{"message_count",1},{"categories",QVariantList{}},
                {"group",group},{"kind","people"},{"unread",unread},{"starred",false}}); };
        h.rows.clear();
        for (int n=0; n<6; ++n) h.rows.append(make(QString("r%1").arg(n),"review",true,"2026-09-14T12:00:00Z"));
        // Three open months of 60, then 37 folded months of 5; Delayed is empty.
        // A first load reads 100 rows, so older pages load while scrolling.
        QJsonArray months; QStringList monthKeys;
        for (int m=0; m<40; ++m) {
            const auto date=QDate(2026,8,28).addMonths(-m);
            const int count = m<3 ? 60 : 5;
            months.append(QJsonObject{{"month",date.toString("yyyy-MM")},{"count",count}}); monthKeys.append("done:"+date.toString("yyyy-MM"));
            for (int n=0; n<count; ++n) h.rows.append(make(QString("d%1-%2").arg(m).arg(n),"done",false,date.toString("yyyy-MM-dd")+"T12:00:00Z"));
        }
        h.summary={{"filtered_group_counts",QJsonObject{{"review",6},{"held",0}}},{"done_months",months}};
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        QStringList qmlWarnings;
        QObject::connect(&qml,&QQmlEngine::warnings,[&](const QList<QQmlError> &errors){ for (const auto &error : errors) qmlWarnings<<error.toString(); });
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Geometry.qml")));
        require(!qml.rootObjects().isEmpty(),"geometry MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"geometry window exposed");
        const auto settle=[&](int ms=150){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,10); };
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"mail view");
        auto *list=find(window->contentItem(),"conversationList"); require(list,"geometry list");
        auto *content=list->property("contentItem").value<QQuickItem*>(); require(content,"list content");
        h.backend.m_openSections=monthKeys.mid(0,3);
        h.backend.m_folder="inbox"; h.backend.m_selected.clear(); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.hasMore();},"the first page loads");
        settle(300);
        int rowHeight=mail->property("rowHeight").toInt();
        const auto real=[&](const char *name){ return list->property(name).toReal(); };
        const auto top=[&]{ return real("originY")-real("topMargin"); };
        const auto bottom=[&]{ return std::max(top(), real("originY")+real("contentHeight")+real("bottomMargin")-real("height")); };
        // Headers, empty lines and rows, each one row high: To review's 6 rows,
        // Delayed's empty line, three months of 60 and 37 folded months.
        const auto items=[&](int foldedRows){ return (1+6)+(1+1)+3*(1+60)+37-foldedRows; };
        QStringList problems;
        // The list's length is its items and what is still to load, exactly,
        // and every delegate is one row high.
        const auto exact=[&](const QString &where, int expectedItems){
            const auto count=list->property("count").toInt();
            auto *footer=list->property("footerItem").value<QQuickItem*>();
            const auto unloaded=h.backend.unloadedItems();
            if (!footer || std::abs(footer->height()-unloaded*rowHeight)>0.5) problems<<where+": the footer is not what is still to load";
            if (std::abs(real("contentHeight")-(count+unloaded)*rowHeight)>0.5) problems<<QString("%1: length %2 for %3 items and %4 to load").arg(where).arg(real("contentHeight")).arg(count).arg(unloaded);
            if (expectedItems>=0 && count+unloaded!=expectedItems) problems<<QString("%1: %2 items and %3 to load, not %4").arg(where).arg(count).arg(unloaded).arg(expectedItems);
            // Each item near the view is where its index puts it from the
            // list's origin, so the list's estimate of what it has not built is
            // exact. (Qt keeps its current item, index 0, alive far away.)
            for (auto *item : content->childItems()) {
                const auto index=item->property("index");
                if (!index.isValid() || index.toInt()<0 || !item->isVisible()) continue;
                if (item->y()+item->height()<real("contentY")-real("height") || item->y()>real("contentY")+2*real("height")) continue;
                if (std::abs(item->height()-rowHeight)>0.5) { problems<<QString("%1: item %2 is %3 high").arg(where).arg(index.toInt()).arg(item->height()); break; }
                if (std::abs(item->y()-real("originY")-index.toInt()*rowHeight)>0.5) { problems<<QString("%1: item %2 at %3 from origin %4").arg(where).arg(index.toInt()).arg(item->y()).arg(real("originY")); break; }
            }
        };
        // Every frame: the view is filled, inside its range, and no header is
        // painted twice.
        int frames=0;
        const auto frame=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{
            ++frames;
            const auto viewTop=list->mapToScene({0,0}).y(), viewBottom=viewTop+list->height();
            qreal covered=0; QHash<QString,int> headers;
            std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){
                if (!item->isVisible() || item->opacity()==0) return;
                const auto name=item->objectName();
                const bool delegate=item->parentItem()==content && item->property("index").isValid() && item->property("index").toInt()>=0;
                const bool placeholder=item->parentItem() && item->parentItem()->objectName()=="unloadedRows" && item->property("place").isValid();
                if (delegate || placeholder) { const auto y=item->mapToScene({0,0}).y(); covered+=std::max<qreal>(0,std::min(viewBottom,y+item->height())-std::max(viewTop,y)); }
                if (name.startsWith("mailGroup_") && item->height()>0) { const auto y=item->mapToScene({0,0}).y(); if (y+item->height()>viewTop && y<viewBottom) ++headers[name]; }
                for (auto *child : item->childItems()) walk(child); };
            walk(list);
            if (covered < std::min<qreal>(list->height(), real("contentHeight")+real("topMargin"))-1) problems<<QString("frame %1: only %2 of the view painted at %3").arg(frames).arg(covered).arg(real("contentY"));
            for (auto it=headers.cbegin(); it!=headers.cend(); ++it) if (it.value()>1) problems<<it.key()+" painted twice";
            if (!list->property("moving").toBool() && (real("contentY")<top()-0.5 || real("contentY")>bottom()+0.5)) problems<<QString("frame %1: at %2 outside %3..%4").arg(frames).arg(real("contentY")).arg(top()).arg(bottom());
        });
        const auto check=[&](const char *step){ if (!problems.isEmpty()) { problems.removeDuplicates(); qWarning()<<problems.mid(0,12); require(false,step); } };
        exact("first page",items(0));
        check("the first page is its full length");
        const auto header=[&](const QString &key){ return find(window->contentItem(),"mailGroup_"+key); };
        const auto toggle=[&](const QString &key){ QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,key),Q_ARG(QVariant,QVariant::fromValue(header(key)))); };
        // Folding the month the loaded rows end in, with older pages still to
        // load: its header stays in place in every frame, the list is shorter
        // by its rows alone, and opening it brings them back.
        {
            QMetaObject::invokeMethod(list,"positionViewAtIndex",Q_ARG(int,h.backend.listIndex("section:"+monthKeys[1])),Q_ARG(int,1)); settle(200);
            list->setProperty("contentY",real("contentY")-200); settle(200);
            auto *item=header(monthKeys[1]); require(item && item->isVisible() && h.backend.hasMore(),"the last loaded month's header is on screen, older pages to load");
            const auto at=item->mapToScene({0,0}).y();
            const auto held=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{
                auto *now=header(monthKeys[1]);
                if (!now || !now->isVisible()) problems<<"the folded month's header left the list";
                else if (std::abs(now->mapToScene({0,0}).y()-at)>0.5) problems<<QString("the folded month's header moved to %1 from %2").arg(now->mapToScene({0,0}).y()).arg(at);
            });
            toggle(monthKeys[1]); settle(400);
            exact("folded with pages to load",items(60));
            toggle(monthKeys[1]); settle(400);
            exact("opened with pages to load",items(0));
            QObject::disconnect(held);
            check("folding the last loaded month keeps its header while older pages load");
            list->setProperty("contentY",top()); settle(200);
        }
        // The first header starts the list, with no room above it.
        auto *review=find(window->contentItem(),"mailGroup_review"); require(review,"To review header");
        require(std::abs(review->mapToItem(list,{0,0}).y())<0.5 && std::abs(real("contentY")-top())<0.5,"the first header is at the top of the list");
        // Scrolled from top to bottom, older pages loading on the way: the
        // length never changes.
        for (qreal y=top(); y<bottom(); y+=330) { list->setProperty("contentY",y); settle(90); exact(QString("scrolled to %1").arg(y),items(0)); }
        list->setProperty("contentY",bottom()); await([&]{return !h.backend.m_loading && !h.backend.hasMore();},"every page loads"); settle(200);
        exact("at the end",items(0));
        check("scrolling and paging keep the list its full length");
        // Folding a month while scrolled down: its header stays where it was,
        // the list is shorter by its rows alone, and scrolling on paints every frame.
        QMetaObject::invokeMethod(list,"positionViewAtIndex",Q_ARG(int,h.backend.listIndex("section:"+monthKeys[1])),Q_ARG(int,1)); settle(200);
        list->setProperty("contentY",real("contentY")-300); settle(200);
        auto *month=header(monthKeys[1]); require(month && month->isVisible(),"the month's header is on screen");
        const auto at=month->mapToScene({0,0}).y();
        QList<qreal> places;
        const auto follow=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{ if (auto *item=header(monthKeys[1])) places<<item->mapToScene({0,0}).y(); else places<<-1; });
        toggle(monthKeys[1]); settle(400);
        exact("folded while scrolled",items(60));
        for (const auto y : places) if (std::abs(y-at)>0.5) { problems<<QString("the folded month's header went from %1 to %2").arg(at).arg(y); break; }
        for (qreal y=real("contentY"); y<bottom(); y+=90) { list->setProperty("contentY",y); settle(20); }
        list->setProperty("contentY",bottom()); settle(100);
        for (qreal y=bottom(); y>top(); y-=250) { list->setProperty("contentY",y); settle(20); }
        exact("scrolled after folding",items(60));
        check("folding a month while scrolled keeps its header and never blanks the view");
        QMetaObject::invokeMethod(list,"positionViewAtIndex",Q_ARG(int,h.backend.listIndex("section:"+monthKeys[1])),Q_ARG(int,1)); settle(200);
        list->setProperty("contentY",real("contentY")-300); settle(200);
        const auto openAt=header(monthKeys[1])->mapToScene({0,0}).y(); places.clear();
        toggle(monthKeys[1]); settle(400);
        exact("opened again",items(0));
        for (const auto y : places) if (std::abs(y-openAt)>0.5) { problems<<QString("the opened month's header went from %1 to %2").arg(openAt).arg(y); break; }
        QObject::disconnect(follow);
        check("opening the month again keeps its header and the list's full length");
        // A fold and an opening paint within a frame or two of the click.
        const int allRows=shownRows(h.backend);
        for (const bool fold : {true, false}) {
            bool painted=false; const auto paintedFrame=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{ painted=true; });
            QElapsedTimer t; t.start(); toggle(monthKeys[1]);
            while (!painted || shownRows(h.backend)!=(fold ? allRows-60 : allRows)) { QCoreApplication::processEvents(QEventLoop::AllEvents,1); if (t.elapsed()>2000) break; }
            QObject::disconnect(paintedFrame);
            qInfo()<<(fold ? "fold" : "open")<<"of 60 rows painted after"<<t.elapsed()<<"ms";
            require(t.elapsed()<100,"a month folds or opens within 100 ms");
            settle(400);
        }
        // Folding while at the very end: the view stays filled, at the end.
        list->setProperty("contentY",bottom()); settle(200);
        h.backend.setSectionCollapsed(monthKeys[2],true); settle(400);
        exact("folded at the end",items(60));
        require(std::abs(real("contentY")-bottom())<0.5,"the list stays at its end");
        h.backend.setSectionCollapsed(monthKeys[2],false); settle(400);
        exact("opened at the end",items(0));
        check("folding at the end of the list never leaves it past its end");
        // Folding every month and opening them again, quickly.
        for (int round=0; round<3; ++round) {
            h.backend.setSections({},{}); settle(round ? 40 : 300);
            h.backend.setSections({},monthKeys.mid(0,3)); settle(round ? 40 : 300);
        }
        settle(500); exact("after quick folds",items(0));
        check("quick folds leave the list exact");
        // New mail arriving while scrolled down: the rows on screen stay put
        // and the list grows by the new row alone.
        {
            QMetaObject::invokeMethod(list,"positionViewAtIndex",Q_ARG(int,h.backend.listIndex("d0-30")),Q_ARG(int,1)); settle(200);
            auto *row=find(window->contentItem(),"conversationRow_d0-30"); require(row && row->isVisible(),"a month row is on screen");
            const auto at=row->mapToScene({0,0}).y();
            h.rows.insert(0,make("arrived","review",true,"2026-09-15T12:00:00Z"));
            auto counts=h.summary["filtered_group_counts"].toObject(); counts["review"]=7; h.summary["filtered_group_counts"]=counts;
            h.backend.loadConversations(false,true); await([&]{return !h.backend.m_loading && h.backend.listIndex("arrived")>=0;},"new mail arrives"); settle(200);
            exact("new mail",items(0)+1);
            row=find(window->contentItem(),"conversationRow_d0-30");
            require(row && std::abs(row->mapToScene({0,0}).y()-at)<0.5,"the rows on screen stay where they were");
            // Compact rows: every item is one compact row high, still exact.
            h.backend.m_preferences["density"]="compact"; emit h.backend.preferencesChanged(); settle(300);
            require(mail->property("rowHeight").toInt()<rowHeight,"compact rows are shorter");
            const int comfortable=rowHeight;
            rowHeight=mail->property("rowHeight").toInt();
            exact("compact",items(0)+1);
            h.backend.m_preferences.remove("density"); emit h.backend.preferencesChanged(); settle(300);
            rowHeight=comfortable;
            exact("comfortable again",items(0)+1);
            h.rows.removeFirst(); counts["review"]=6; h.summary["filtered_group_counts"]=counts;
            h.backend.loadConversations(false,true); await([&]{return !h.backend.m_loading && h.backend.listIndex("arrived")<0;},"the new mail is filed"); settle(200);
            check("new mail and density changes keep the list exact");
        }
        // A kept row that Core no longer returns goes back into its own month,
        // where it was, even with new mail above; folding its month hides it
        // and its header is painted once.
        const auto rowsNow=h.backend.conversations();
        int keptIndex=-1; for (int i=0;i<rowsNow.size();++i) if (rowsNow[i].toMap().value("id")=="d1-10") keptIndex=i;
        require(keptIndex>0,"the kept row is listed");
        h.backend.m_selected="d1-10"; h.backend.m_pinnedRow=rowsNow[keptIndex].toMap(); h.backend.m_pinnedIndex=keptIndex;
        h.rows.erase(std::remove_if(h.rows.begin(),h.rows.end(),[](const QVariant &row){ return row.toMap().value("id")=="d1-10"; }),h.rows.end());
        h.rows.insert(6,make("fresh","done",false,"2026-08-29T12:00:00Z"));
        h.backend.loadConversations(false,true); await([&]{return !h.backend.m_loading;},"the kept row's refresh"); settle(200);
        const auto ids=[&]{ QStringList out; for (const auto &row:h.backend.conversations()) out<<row.toMap().value("id").toString(); return out; };
        const auto order=ids();
        require(order.indexOf("d1-10")==order.indexOf("d1-9")+1 && order.indexOf("d1-11")==order.indexOf("d1-10")+1,"the kept row stays in its month, where it was");
        QMetaObject::invokeMethod(list,"positionViewAtIndex",Q_ARG(int,h.backend.listIndex("section:"+monthKeys[1])),Q_ARG(int,1)); settle(200);
        h.backend.setSectionCollapsed(monthKeys[1],true); settle(300);
        auto *keptRow=find(window->contentItem(),"conversationRow_d1-10");
        require(!keptRow || !keptRow->isVisible(),"folding the kept row's month hides it");
        exact("kept row folded",-1);
        h.backend.setSectionCollapsed(monthKeys[1],false); settle(400);
        exact("kept row's month open",-1);
        check("a kept row folds with its month and its header shows once");
        QObject::disconnect(frame);
        // Switching accounts during a search: the next account starts with its own folds.
        h.backend.setQuery("Geometry"); require(h.backend.allSectionsOpen(),"a search opens every month");
        auto second=h.backend.m_allAccounts.first().toMap(); second["id"]="second"; second["name"]="Second"; second["email"]="second@example.test";
        h.backend.m_allAccounts.append(second); h.backend.publishAccounts(h.backend.m_allAccounts);
        h.backend.selectAccount("second");
        require(!h.backend.allSectionsOpen() && h.backend.collapsedGroups().isEmpty(),"another account does not keep the search's open months");
        if (!qmlWarnings.isEmpty()) { qWarning()<<qmlWarnings.mid(0,6); require(false,"the list has no QML warnings"); }
        require(frames>40,"the list painted while it changed");
        qInfo()<<"The Inbox list is its exact length through scrolling, paging and folds, and never blank, over"<<frames<<"frames.";
    }
    // Folding or opening a month repaints only what changes: in every frame
    // the rows above it stay painted in place, by the same delegates, and the
    // list never dims.
    static void monthFoldFrames() {
        Harness h;
        h.filterCollapsed = true;
        const auto make=[](const QString &id,const QString &group,bool unread,const QString &at){
            return QVariant(QVariantMap{{"id",id},{"subject","Month "+id},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"latest_at",at},
                {"message_ids",QVariantList{id+"-original"}},{"revision",3},{"message_count",1},{"categories",QVariantList{}},
                {"group",group},{"kind","people"},{"unread",unread},{"starred",false}}); };
        h.rows.clear();
        for (int n=0; n<4; ++n) h.rows.append(make(QString("r%1").arg(n),"review",true,"2026-09-14T12:00:00Z"));
        QJsonArray months; QStringList monthKeys;
        // Three open months, then many folded ones, as years of mail are.
        for (int m=0; m<30; ++m) {
            const auto date=QDate(2026,8,15).addMonths(-m);
            months.append(QJsonObject{{"month",date.toString("yyyy-MM")},{"count",3}}); monthKeys.append("done:"+date.toString("yyyy-MM"));
            for (int n=0; n<3; ++n) h.rows.append(make(QString("d%1-%2").arg(m).arg(n),"done",false,date.toString("yyyy-MM-dd")+"T12:00:00Z"));
        }
        h.summary={{"filtered_group_counts",QJsonObject{{"review",4},{"held",0}}},{"done_months",months}};
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Folds.qml")));
        require(!qml.rootObjects().isEmpty(),"fold MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"fold window exposed");
        const auto settle=[&](int ms=150){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,10); };
        h.backend.m_openSections=monthKeys;
        h.backend.m_folder="inbox"; h.backend.m_selected.clear(); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==h.rows.size();},"every month loads");
        h.backend.setSections({},monthKeys.mid(0,3));
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==4+9;},"older months fold");
        settle(300);
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"mail view");
        auto *list=find(window->contentItem(),"conversationList"); require(list,"fold list");
        // Painted items only: retired delegates can linger hidden.
        const auto paintedRow=[&](const QString &id)->QQuickItem* { QQuickItem *found=nullptr; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(found || !item->isVisible() || item->opacity()==0) return; if(item->objectName()=="conversationRow_"+id && item->height()>0) { found=item; return; } for(auto *c:item->childItems()) walk(c); }; walk(window->contentItem()); return found; };
        QStringList stayers{"r0","r1","r2","r3"};
        QHash<QString, QPair<QQuickItem*, qreal>> baseline;
        for (const auto &id : stayers) { auto *row=paintedRow(id); require(row,"a To review row is painted"); baseline[id]={row,row->mapToScene({0,0}).y()}; }
        // Month headers other than the toggled one, on screen before: they may
        // move, but never vanish for a frame.
        const auto headerY=[&](const QString &key)->qreal { qreal found=-1e9; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(found>-1e9 || !item->isVisible() || item->opacity()==0) return; if(item->objectName()=="mailGroup_"+key && item->height()>0) { found=item->mapToScene({0,0}).y(); return; } for(auto *c:item->childItems()) walk(c); }; walk(window->contentItem()); return found; };
        const auto paintedHeader=[&](const QString &key){ return headerY(key)>-1e9; };
        // Each header's place in every frame of one toggle; it may only be
        // where it started or where it ends.
        QHash<QString, QList<qreal>> places;
        QStringList headers;
        for (const auto &key : monthKeys) if (paintedHeader(key)) headers<<key;
        // The list builds items near the view only; a few month headers are on screen.
        require(headers.size()>=3,"several month headers are on screen");
        QString toggled;
        QStringList problems;
        const auto frame=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{
            for (const auto &key : headers) { const auto y=headerY(key); if (key!=toggled && y<=-1e9) problems<<key+" header missing"; places[key]<<y; }
            if (list->opacity()<1) problems<<"the list dimmed";
            for (const auto &id : stayers) {
                auto *row=paintedRow(id);
                if (!row) problems<<id+" not painted";
                else if (row!=baseline[id].first) problems<<id+" rebuilt";
                else if (std::abs(row->mapToScene({0,0}).y()-baseline[id].second)>1) problems<<id+" moved";
            }
        });
        for (const auto &key : {monthKeys[0], monthKeys[1]}) {
            toggled=key;
            for (const bool late : {false, true}) {
                h.holdLists=late;
                const auto places0=[&]{ QHash<QString,qreal> at; for (const auto &k : headers) at[k]=headerY(k); return at; }();
                places.clear();
                QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,key),Q_ARG(QVariant,QVariant::fromValue(find(window->contentItem(),"mailGroup_"+key))));
                settle(late ? 400 : 300); if (late) { h.releaseLists(); settle(300); }
                for (const auto &k : headers) {
                    if (k==key) continue;
                    const auto end=headerY(k);
                    for (const auto y : places.value(k)) if (std::abs(y-places0[k])>1 && std::abs(y-end)>1) { problems<<QString("%1 passed through %2 between %3 and %4").arg(k).arg(y).arg(places0[k]).arg(end); break; }
                }
                QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,key),Q_ARG(QVariant,QVariant::fromValue(find(window->contentItem(),"mailGroup_"+key))));
                settle(late ? 400 : 300); if (late) { h.releaseLists(); settle(300); }
            }
        }
        h.holdLists=false;
        QObject::disconnect(frame);
        if (!problems.isEmpty()) { problems.removeDuplicates(); qWarning()<<problems.mid(0,12); require(false,"folding a month repaints only what changes"); }
        // Scrolled into To review, with a thread open: its band stays stuck
        // at the top through every frame of a fold below.
        h.backend.selectConversation("r2"); settle(300);
        list->setProperty("contentY",list->property("originY").toReal()+60); settle(300);
        auto *sticky=find(window->contentItem(),"mailStickyGroup"); require(sticky && sticky->isVisible(),"To review's band is stuck");
        QStringList bandProblems;
        const auto bandFrame=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{ if (!sticky->isVisible()) bandProblems<<"band hidden"; if (list->opacity()<1) bandProblems<<"list dimmed"; });
        for (int round=0; round<2; ++round) {
            QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,monthKeys[1]),Q_ARG(QVariant,QVariant::fromValue(find(window->contentItem(),"mailGroup_"+monthKeys[1]))));
            settle(300);
        }
        QObject::disconnect(bandFrame);
        if (!bandProblems.isEmpty()) { bandProblems.removeDuplicates(); qWarning()<<bandProblems; require(false,"the stuck band holds while a month folds below"); }
        // Scrolled far down with many months open: no frame of a fold or an
        // opening is blank.
        h.backend.setSections({},monthKeys.mid(0,12));
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==4+36;},"twelve months open");
        settle(300);
        QMetaObject::invokeMethod(list,"positionViewAtEnd"); settle(300);
        const auto shownInList=[&]{ int n=0; const auto top=list->mapToScene({0,0}).y(), bottom=top+list->height();
            std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(!item->isVisible() || item->opacity()==0) return;
                const auto name=item->objectName();
                if ((name.startsWith("conversationRow_") || name.startsWith("mailGroup_")) && item->height()>0) { const auto y=item->mapToScene({0,0}).y(); if (y+item->height()>top && y<bottom) ++n; }
                for(auto *c:item->childItems()) walk(c); };
            walk(list); return n; };
        int fewest=shownInList(); require(fewest>4,"rows fill the scrolled list");
        QStringList blank;
        const auto blankFrame=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{ const auto n=shownInList(); fewest=std::min(fewest,n); if (n==0) blank<<"blank frame"; if (list->opacity()<1) blank<<"list dimmed"; });
        for (const auto &key : {monthKeys[11], monthKeys[8], monthKeys[5]}) {
            for (int round=0; round<2; ++round) {
                auto *header=find(window->contentItem(),"mailGroup_"+key);
                QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,key),Q_ARG(QVariant,QVariant::fromValue(header)));
                settle(350);
            }
        }
        QObject::disconnect(blankFrame); qInfo() << "fewest painted in the scrolled list" << fewest;
        if (!blank.isEmpty()) { blank.removeDuplicates(); qWarning()<<blank<<"fewest painted"<<fewest; require(false,"folding while scrolled down never blanks the list"); }
        // Every month above folded, one open below them, as when reading an
        // old month: folding and opening it keeps each header above as it is,
        // the same item in the same place in every frame.
        h.backend.setSections({},QStringList{monthKeys[10]});
        await([&]{return !h.backend.m_loading;},"one older month open below folded ones");
        // As after a launch: the folded months above were never loaded.
        h.backend.m_sectionRows.clear(); h.backend.loadConversations(false,true);
        await([&]{return !h.backend.m_loading && shownRows(h.backend)==4+3;},"only the open month's rows are listed");
        settle(300);
        // The open month's first row at the bottom of the view, the folded
        // headers above it on screen.
        QMetaObject::invokeMethod(list,"positionViewAtIndex",Q_ARG(int,h.backend.listIndex("d10-0")),Q_ARG(int,2)); settle(300);
        const auto headerItem=[&](const QString &key)->QQuickItem* { QQuickItem *found=nullptr; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(found || !item->isVisible() || item->opacity()==0) return; if(item->objectName()=="mailGroup_"+key && item->height()>0) { found=item; return; } for(auto *c:item->childItems()) walk(c); }; walk(window->contentItem()); return found; };
        QHash<QString, QPair<QQuickItem*, qreal>> above;
        for (int m=0; m<10; ++m) if (auto *item=headerItem(monthKeys[m])) above[monthKeys[m]]={item,item->mapToScene({0,0}).y()};
        require(above.size()>=3,"folded month headers above the open month are on screen");
        QStringList rebuilt;
        const auto aboveFrame=QObject::connect(window,&QQuickWindow::frameSwapped,window,[&]{
            for (auto it=above.cbegin(); it!=above.cend(); ++it) {
                auto *item=headerItem(it.key());
                if (!item) rebuilt<<it.key()+" missing";
                else if (item!=it.value().first) rebuilt<<it.key()+" rebuilt";
                else if (std::abs(item->mapToScene({0,0}).y()-it.value().second)>1) rebuilt<<it.key()+" moved";
            }
        });
        for (int round=0; round<2; ++round) {
            QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,monthKeys[10]),Q_ARG(QVariant,QVariant::fromValue(find(window->contentItem(),"mailGroup_"+monthKeys[10]))));
            settle(350);
        }
        // Folding and opening it again and again never paints a header twice.
        for (int round=0; round<6; ++round) {
            QMetaObject::invokeMethod(mail,"toggleGroup",Q_ARG(QVariant,monthKeys[10]),Q_ARG(QVariant,QVariant::fromValue(find(window->contentItem(),"mailGroup_"+monthKeys[10]))));
            settle(round%2 ? 350 : 60);
        }
        settle(400);
        QObject::disconnect(aboveFrame);
        if (!rebuilt.isEmpty()) { rebuilt.removeDuplicates(); qWarning()<<rebuilt.mid(0,10); require(false,"folding a month keeps the folded headers above it"); }
        {
            QHash<QString,int> painted; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(!item->isVisible() || item->opacity()==0) return; if(item->objectName().startsWith("mailGroup_") && item->height()>0 && item->objectName()!="mailGroup_review") ++painted[item->objectName()]; for(auto *c:item->childItems()) walk(c); };
            walk(list);
            for (auto it=painted.cbegin(); it!=painted.cend(); ++it) if (it.value()!=1) { qWarning()<<it.key()<<"painted"<<it.value(); require(false,"each month header is painted once"); }
        }
        qInfo()<<"Folding and opening a month keeps the rows above it painted in place.";
    }
    // Reply after discarding the thread's reply reopens it, and it stays open
    // through later refreshes; typing revives it.
    static void replyAfterDiscard() {
        Harness h;
        h.drafts["a"] = {{"account","personal"},{"draft_id","a"},{"conversation_id","a"},{"mode","reply"},{"source_message_id","a-original"},{"legacy_reply",true},
            {"revision",0},{"discarded",false},{"body",""},{"subject","Re: Labeled message a"},{"to_recipients",QJsonArray{QJsonObject{{"email","sender@example.test"},{"name","Sender"}}}},
            {"cc_recipients",QJsonArray{}},{"bcc_recipients",QJsonArray{}},{"attachment_ids",QJsonArray{}},{"attachments",QJsonArray{}}};
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Reply.qml")));
        require(!qml.rootObjects().isEmpty(),"reply MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"reply window exposed");
        const auto settle=[&](int ms){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,10); };
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"mail view");
        const auto press=[&](const QString &name){ auto *item=find(window->contentItem(),name); require(item && item->isVisible() && item->isEnabled(),qPrintable("visible control "+name)); require(QMetaObject::invokeMethod(item,"clicked"),"control handler"); };
        const auto panelShown=[&]{ auto *panel=find(window->contentItem(),"replyPanel"); return mail->property("replyOpen").toBool() && panel && panel->isVisible() && panel->height()>0; };
        h.backend.m_folder="inbox"; h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==3;},"Inbox loads");
        h.backend.selectConversation("a");
        await([&]{return h.backend.m_draft.value("draft_id")=="a" && !h.backend.m_conversation.value("messages").toList().isEmpty();},"the thread and its reply load");
        press("replyButton"); await(panelShown,"Reply opens the thread's reply");
        h.backend.editDraft("Half a thought"); settle(300);
        press("discardReply"); await([&]{return find(window->contentItem(),"confirmDiscardDraft");},"discard asks first");
        settle(100); press("confirmDiscardDraft");
        await([&]{return h.backend.draftStatus()=="Draft discarded" && !mail->property("replyOpen").toBool();},"discard closes the reply");
        settle(300);
        press("replyButton");
        await(panelShown,"Reply after a discard opens the reply");
        // Sync refreshes the draft while it is open; it stays open.
        QElapsedTimer held; held.start();
        while (held.elapsed()<1500) {
            emit h.backend.draftChanged(); QCoreApplication::processEvents(QEventLoop::AllEvents,10);
            require(panelShown(),"the reopened reply stays open");
        }
        h.backend.editDraft("Second thought");
        await([&]{return h.drafts["a"]["body"]=="Second thought";},"typing saves the reply");
        require(!h.drafts["a"]["discarded"].toBool() && h.backend.m_draft.value("draft_id")=="a" && !h.methods.contains("draft_create"),"the same reply is revived, with no new draft");
        require(panelShown(),"the revived reply is still open");
        // Discarding it again closes it again.
        settle(300); press("discardReply"); await([&]{return find(window->contentItem(),"confirmDiscardDraft");},"discard asks again");
        settle(100); press("confirmDiscardDraft");
        await([&]{return h.backend.draftStatus()=="Draft discarded" && !mail->property("replyOpen").toBool();},"a second discard closes the reply");
        const auto shown=[&](const QString &name){ auto *item=find(window->contentItem(),name); return item && item->isVisible(); };
        const auto prominent=[&](const QString &name){ return find(window->contentItem(),name)->property("prominent").toBool(); };
        require(shown("replyButton") && shown("forwardButton") && prominent("replyButton"),"closing leaves Reply and Forward, Reply first");
        // Reply, then Forward: the words move into the forward, the reply is
        // retired, and the thread stays on screen throughout.
        press("replyButton"); await(panelShown,"Reply reopens");
        require(shown("replyButton") && shown("forwardButton"),"the buttons stay while replying");
        h.backend.editDraft("Half a thought"); await([&]{return h.drafts["a"]["body"]=="Half a thought";},"the reply saves");
        QPointer<QQuickItem> member=find(window->contentItem(),"messageMember_a-original"); require(member,"the thread's message");
        auto *reader=find(window->contentItem(),"reader"); require(reader,"reader");
        bool blanked=false; QMetaObject::Connection watch=QObject::connect(&h.backend,&MailBackend::conversationChanged,[&]{ if (h.backend.m_conversation.value("messages").toList().isEmpty()) blanked=true; });
        press("forwardButton");
        await([&]{return h.backend.m_draft.value("mode")=="forward" && panelShown();},"Forward turns the reply into a forward");
        await([&]{return h.drafts["a"]["discarded"].toBool();},"the reply is retired once the forward is saved");
        QObject::disconnect(watch);
        const auto body=h.backend.m_draft.value("body").toString();
        require(body.startsWith("Half a thought") && body.contains("Forwarded message"),"the words written come along, above the forwarded message");
        require(!blanked && member && find(window->contentItem(),"messageMember_a-original")==member.data(),"the thread is not blanked or rebuilt");
        require(prominent("forwardButton") && !prominent("replyButton") && shown("replyButton"),"Forward is marked, Reply is still there");
        // Discard goes back to the buttons.
        settle(300); press("discardReply"); await([&]{return find(window->contentItem(),"confirmDiscardDraft");},"discard asks");
        settle(100); press("confirmDiscardDraft");
        await([&]{return !mail->property("replyOpen").toBool() && !panelShown();},"Discard closes the forward");
        require(shown("replyButton") && prominent("replyButton"),"back to Reply and Forward");
        // A new message discarded returns to the thread it was started from.
        QMetaObject::invokeMethod(mail,"newMessage");
        await([&]{return h.backend.m_draft.value("mode")=="new" && panelShown();},"New message opens");
        settle(200); press("discardReply");
        await([&]{return h.backend.m_selected=="a" && !h.backend.m_conversation.value("messages").toList().isEmpty() && !mail->property("replyOpen").toBool();},"discarding it returns to the thread");
    }
    // Sync refreshes the list while a Done conversation is open: a new message
    // moves it to the top. The list shows it once, under one month header.
    // The list is its full length before older pages load, so arriving pages
    // never move the scrollbar, and a jump to the end loads through to it
    // without a loading line.
    // Text mode shows a forward as a label, a header block, the forwarded
    // text and the history it quotes, each quote level indented with a bar.
    static void forwardLayout() {
        Harness h;
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:900; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Forward.qml")));
        require(!qml.rootObjects().isEmpty(),"forward MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"forward window exposed");
        auto *mail=find(window->contentItem(),"mailWorkspace"); require(mail,"mail view");
        h.backend.m_folder="inbox"; h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && !h.backend.conversations().isEmpty();},"Inbox loads");
        h.backend.selectConversation("a");
        await([&]{return !h.backend.m_conversation.value("messages").toList().isEmpty();},"the thread loads");
        const auto text=QString("Begin forwarded message:\n\n> From: Panel <panel@example.test>\n> Date: Monday\n> To: Team <team@example.test>\n> Subject: Re: Trial\n>\n"
            "> I would like more time once the agreement is signed.\n>\n> Thanks,\n> Sam\n> On Monday, Team <team@example.test> wrote:\n>\n> > Hi Sam,\n> >\n> > Your trial ends tomorrow.\n> > > An older note");
        const auto message=QVariantMap{{"id","fwd"},{"sender_name","Forwarder"},{"sender_email","forwarder@example.test"},{"sent_at","2026-09-14T10:00:00Z"},{"text",text},{"sent",false},{"source_available",false}};
        h.backend.m_conversation={{"id","a"},{"subject","Fwd: Trial"},{"revision",3},{"message_ids",QVariantList{"fwd"}},{"messages",QVariantList{message}}};
        emit h.backend.conversationChanged();
        QQuickItem *label=nullptr,*quote=nullptr,*older=nullptr,*headers=nullptr,*attribution=nullptr,*body=nullptr;
        await([&]{
            label=quote=older=headers=attribution=body=nullptr;
            for(int i=0;i<12;++i){auto *row=find(window->contentItem(),i?QString("messageBodyPart_fwd_%1").arg(i):QString("messageBody_fwd"));if(!row)continue;
                const auto kind=row->property("kind").toString();const int level=row->property("level").toInt();
                if(kind=="forward")label=row;else if(kind=="headers")headers=row;else if(kind=="attribution")attribution=row;else if(kind=="quote"&&level==1)quote=row;else if(kind=="quote"&&level==2)older=row;else if(kind.isEmpty())body=row;}
            return label && headers && body && attribution && quote && older && older->height()>0;},"the forward's rows are laid out");
        require(label->y()<headers->y() && headers->y()<body->y() && body->y()<attribution->y() && attribution->y()<quote->y() && quote->y()<older->y(),"label, headers, text, attribution and quotes read top to bottom");
        require(qAbs(quote->property("leftPadding").toReal()-16)<0.5 && qAbs(older->property("leftPadding").toReal()-32)<0.5 && body->property("leftPadding").toReal()==0,"each quote level indents 16 pixels more");
        int bars=0; for(auto *child:older->childItems()) if(child->width()==2 && child->height()>10) ++bars;
        require(bars==2,"a second-level quote draws a bar for each level");
        require(label->property("font").value<QFont>().capitalization()==QFont::AllUppercase,"the forward label is a small capitalised caption");
        require(body->property("text").toString().contains("Thanks,") && !body->property("text").toString().contains("wrote:"),"the forwarded text stops before the attribution");
        const auto capture=qEnvironmentVariable("FM_FORWARD_CAPTURE");
        if(!capture.isEmpty()){QElapsedTimer t;t.start();while(t.elapsed()<200)QCoreApplication::processEvents(QEventLoop::AllEvents,10);require(window->grabWindow().save(capture),"forward frame saved");}
    }
    static void pagingKeepsLength() {
        Harness h;
        h.pageSize = 100;
        h.rows.clear();
        for (int n=0; n<350; ++n) h.rows.append(QVariantMap{{"id",QString("p%1").arg(n)},{"subject",QString("Paged %1").arg(n)},{"sender_name","Sender"},{"sender_email","sender@example.test"},
            {"latest_at",QDateTime(QDate(2026,9,26),QTime(12,0)).addSecs(-n*600).toUTC().toString(Qt::ISODate)},{"message_ids",QVariantList{QString("p%1-m").arg(n)}},{"revision",3},
            {"message_count",1},{"categories",QVariantList{}},{"group","done"},{"kind","people"},{"unread",false},{"starred",false}});
        h.summary={{"filtered_group_counts",QJsonObject{{"review",0},{"held",0},{"done",350}}},{"done_months",QJsonArray{QJsonObject{{"month","2026-09"},{"count",350}}}}};
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Paging.qml")));
        require(!qml.rootObjects().isEmpty(),"paging MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"paging window exposed");
        const auto settle=[&](int ms=150){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,10); };
        h.backend.m_folder="inbox"; h.backend.m_selected.clear(); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==100;},"the first page loads"); settle();
        auto *list=find(window->contentItem(),"conversationList"); require(list,"paging list");
        const auto length=[&]{ return list->property("contentHeight").toReal(); };
        const auto rowHeight=find(window->contentItem(),"mailWorkspace")->property("rowHeight").toReal();
        const auto full=length();
        require(full > 349*rowHeight,"the list is as long as all 350 rows before they load");
        // Scrolling near the loaded end loads the next page in place. Qt
        // estimates rows not yet built from those on screen, so the length is
        // compared at one scroll position, before and after the page lands.
        h.holdLists=true;
        list->setProperty("contentY",list->property("loadedBottom").toReal()-2*list->height());
        await([&]{return !h.heldLists.isEmpty();},"scrolling asks for the next page"); settle();
        const auto beforePage=length();
        h.releaseLists();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==200;},"the next page loads while scrolling"); settle();
        if (std::abs(length()-beforePage)>2*rowHeight) qWarning()<<"length"<<beforePage<<"became"<<length();
        require(std::abs(length()-beforePage)<=2*rowHeight,"an arriving page leaves the list's length alone");
        // A jump to the end loads through to it.
        list->setProperty("contentY",length()-list->height());
        await([&]{return !h.heldLists.isEmpty();},"a jump to the end asks for the rest"); settle();
        const auto beforeEnd=length();
        while (h.backend.conversations().size()<350) { h.releaseLists(); settle(50); require(h.backend.m_loading || h.backend.conversations().size()==350 || !h.heldLists.isEmpty(),"loading through to the end continues"); }
        h.holdLists=false; await([&]{return !h.backend.m_loading;},"a jump to the end loads every row"); settle(300);
        if (std::abs(length()-beforeEnd)>4*rowHeight) qWarning()<<"length"<<beforeEnd<<"became"<<length();
        require(std::abs(length()-beforeEnd)<=4*rowHeight,"the fully loaded list is as long as it was sized");
        bool loadingText=false; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if (item->isVisible() && item->property("text").toString()==QStringLiteral("Loading…")) loadingText=true; for (auto *c:item->childItems()) walk(c); };
        walk(window->contentItem());
        require(!loadingText,"paging never shows a loading line");
        qInfo()<<"Older pages fill a list already its full length.";
    }
    // Switching between Inbox and a long Sent list shows the new rows within
    // a few frames; grouping Sent by day once per list, not per header, keeps
    // it from growing with every page scrolled through.
    static void folderSwitchSpeed() {
        Harness h;
        const auto make=[](int n,bool sent){
            const auto id=QString(sent ? "s%1" : "i%1").arg(n);
            const auto at=QDateTime(QDate(2026,9,26),QTime(12,0)).addSecs(-qint64(n)*3600*9).toUTC().toString(Qt::ISODate);
            QVariantMap row{{"id",id},{"subject","A subject line for row "+id},{"sender_name","Person "+QString::number(n%17)},{"sender_email",QString("p%1@example.test").arg(n%17)},{"latest_at",at},
                {"message_ids",QVariantList{id+"-m"}},{"revision",3},{"message_count",1+n%4},{"categories",QVariantList{}},{"group",n<8 ? "review" : "done"},
                {"kind",n%3 ? "people" : "automated"},{"unread",n<8},{"starred",false},{"snippet","A preview of the message body"}};
            if(sent) { row["recipient"]="teammate@example.test"; row["group"]="done"; }
            return QVariant(row); };
        QVariantList inbox, sent;
        for(int n=0;n<100;++n) inbox.append(make(n,false));
        for(int n=0;n<400;++n) sent.append(make(n,true));
        h.rows=inbox;
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; flags:Qt.WindowDoesNotAcceptFocus; color:Theme.canvas; width:1440; height:900; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Speed.qml")));
        require(!qml.rootObjects().isEmpty(),"folder switch MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"folder switch window exposed");
        const auto settle=[&](int ms){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,5); };
        h.backend.m_folder="inbox"; h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==100;},"Inbox loads"); settle(300);
        // The first visit also compiles Sent's parts; later visits are timed.
        for(int round=0;round<3;++round) for(const QString folder:{"sent","inbox"}) {
            h.rows = folder=="sent" ? sent : inbox;
            const auto first = folder=="sent" ? QString("s0") : QString("i0");
            QElapsedTimer t; t.start();
            h.backend.setFolder(folder);
            while(!(h.backend.conversations().value(0).toMap().value("id")==first) && t.elapsed()<5000) QCoreApplication::processEvents(QEventLoop::AllEvents,1);
            if(t.elapsed()>=5000) qWarning()<<"switch to"<<folder<<"shows"<<h.backend.conversations().size()<<"rows, first"<<h.backend.conversations().value(0).toMap().value("id")<<"folder"<<h.backend.folder()<<"loading"<<h.backend.m_loading;
            // Rendering the frame here, rather than waiting for one, times the
            // same layout and paint while the display sleeps.
            window->grabWindow();
            qInfo()<<"round"<<round<<folder<<"first frame after"<<t.elapsed()<<"ms";
            if(round>0) require(t.elapsed()<300,"a folder switch paints its rows within 300 ms");
            settle(300);
        }
    }
    // A found photo fills the whole circle at every size, not just the
    // corner of the picture that fits. The circle cut only exists on a GPU
    // renderer; the software one draws the plain picture.
    static void photoFillsCircle(bool gpuOnly=false) {
        QTemporaryDir copy; copyModule(copy);
        const QColor quarters[]={Qt::red,Qt::green,Qt::blue,Qt::yellow};
        QImage photo(64,64,QImage::Format_RGB32);
        for(int y=0;y<64;++y) for(int x=0;x<64;++x) photo.setPixelColor(x,y,quarters[(y>=32)*2+(x>=32)]);
        require(photo.save(copy.filePath("photo.png")),"a photo with one colour per quarter");
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path());
        QQmlComponent service(&qml);
        service.setData("import QtQuick\nQtObject { property QtObject htmlContent: QtObject { property int pictureRevision: 0; function senderPicture(email, kind) { return {state:'ready', source:'photo', label:'Profile photo', url:'"+QUrl::fromLocalFile(copy.filePath("photo.png")).toString().toUtf8()+"'} } } }",QUrl());
        // Owned by the engine, so it outlives the window that reads it.
        auto *backend=service.create(); require(backend!=nullptr,"stand-in picture service"); backend->setParent(&qml);
        qml.rootContext()->setContextProperty("backend",backend);
        qml.loadData("import QtQuick\nimport SelectionTest 1.0\nWindow { visible:true; flags:Qt.WindowDoesNotAcceptFocus; color:'black'; width:200; height:80; Row { x:10; y:10; spacing:10; Repeater { model:[28,18,34,36]; SenderAvatar { objectName:'photo'+modelData; diameter:modelData; name:'Photo'; email:'photo@example.test'; kind:'people' } } } }",QUrl::fromLocalFile(copy.filePath("Photo.qml")));
        require(!qml.rootObjects().isEmpty(),"photo avatars load");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"photo window exposed");
        const bool gpu=window->rendererInterface()->graphicsApi()!=QSGRendererInterface::Software;
        if(gpuOnly && !gpu) { qInfo()<<"Not checked: no GPU renderer here, so photos are not cut to a circle."; return; }
        await([&]{for(int d:{28,18,34,36}){auto *item=find(window->contentItem(),QString("photo%1").arg(d)); if(!item || !item->property("imageShown").toBool()) return false;} return true;},"every photo loads");
        QElapsedTimer t; t.start(); while(t.elapsed()<200) QCoreApplication::processEvents(QEventLoop::AllEvents,10);
        const auto frame=window->grabWindow(); const qreal ratio=frame.devicePixelRatio();
        // The dominant channels of a quarter drawn at 75% on black.
        const auto matches=[](const QColor &seen,const QColor &want){ for(auto [s,w]:{std::pair{seen.red(),want.red()},{seen.green(),want.green()},{seen.blue(),want.blue()}}) if(w ? s<140 : s>60) return false; return true; };
        for(int d:{28,18,34,36}) {
            auto *item=find(window->contentItem(),QString("photo%1").arg(d));
            for(int quarter=0;quarter<4;++quarter) {
                const auto at=item->mapToScene(QPointF(d*(quarter%2 ? .75 : .25),d*(quarter/2 ? .75 : .25)))*ratio;
                const auto seen=frame.pixelColor(at.toPoint());
                if(!matches(seen,quarters[quarter])) qWarning()<<"diameter"<<d<<"quarter"<<quarter<<seen;
                require(matches(seen,quarters[quarter]),"each quarter of the photo shows in its quarter of the circle");
            }
        }
        qInfo()<<"A photo fills its circle at every size on the"<<(gpu ? "GPU" : "software")<<"renderer.";
    }
    static void refreshedRowMoves() {
        Harness h;
        const auto make=[](const QString &id,const QString &at,int count){
            return QVariant(QVariantMap{{"id",id},{"subject","Thread "+id},{"sender_name","Sender "+id},{"sender_email","sender@example.test"},{"latest_at",at},
                {"message_ids",QVariantList{id+"-original"}},{"revision",3},{"message_count",count},{"categories",QVariantList{}},
                {"group","done"},{"section","done:2026-09"},{"kind","people"},{"unread",false},{"starred",false}}); };
        h.rows.clear();
        for (int n=0; n<24; ++n) h.rows.append(make(QString("d%1").arg(n),QString("2026-09-26T%1:00:00Z").arg(23-n,2,10,QChar('0')),n==1 ? 5 : 1));
        h.summary={{"filtered_group_counts",QJsonObject{{"review",0},{"held",0},{"done",24}}},{"done_months",QJsonArray{QJsonObject{{"month","2026-09"},{"count",24}}}}};
        QTemporaryDir copy; copyModule(copy);
        QQmlApplicationEngine qml; FmIconItem::registerType();
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&h.backend);
        qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport SelectionTest 1.0\nApplicationWindow { visible:true; color:Theme.canvas; width:1280; height:800; property bool singleKeysAllowed:true; property bool searchPopupOpen:false; MailView { anchors.fill:parent } }",QUrl::fromLocalFile(copy.filePath("Moved.qml")));
        require(!qml.rootObjects().isEmpty(),"moved-row MailView loads");
        auto *window=qobject_cast<QQuickWindow *>(qml.rootObjects().first());
        await([&]{return window->isExposed();},"moved-row window exposed");
        const auto settle=[&](int ms=150){ QElapsedTimer t; t.start(); while(t.elapsed()<ms) QCoreApplication::processEvents(QEventLoop::AllEvents,10); };
        h.backend.m_folder="inbox"; h.backend.m_selected.clear(); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading && h.backend.conversations().size()==24;},"the month loads");
        h.backend.selectConversation("d1"); settle(300);
        // Only painted items count: retired delegates can linger hidden.
        const auto painted=[&](const QString &name){ int found=0; std::function<void(QQuickItem*)> walk=[&](QQuickItem *item){ if(!item->isVisible() || item->opacity()==0) return; if(item->objectName()==name && item->height()>0) ++found; for(auto *c:item->childItems()) walk(c); }; walk(window->contentItem()); return found; };
        require(painted("conversationRow_d1")==1 && painted("mailGroup_done:2026-09")==1,"one row and one month header before the refresh");
        const auto check=[&](const QString &when){
            const int rows=painted("conversationRow_d1"), headers=painted("mailGroup_done:2026-09");
            if (rows!=1 || headers!=1) qWarning()<<when<<"rows"<<rows<<"headers"<<headers;
            require(rows==1,"the open conversation shows once");
            require(headers==1,"its month shows one header");
        };
        // The open thread starts first; mail arriving above it pushes it
        // down, then a reply brings it back to the top.
        auto first=h.rows.takeAt(1); h.rows.prepend(first);
        {auto row=h.rows[0].toMap(); row["latest_at"]="2026-09-27T00:12:00Z"; h.rows[0]=row;}
        h.backend.refreshState(true,false,true); settle(400); check("opened thread first");
        for (int round=0; round<3; ++round) {
            h.rows.prepend(make(QString("n%1").arg(round),QString("2026-09-27T0%1:23:00Z").arg(round),1));
            h.backend.refreshState(true,false,true); settle(400); check(QString("mail arrived above, round %1").arg(round));
            auto moved=h.rows.takeAt(1).toMap();
            moved["latest_at"]=QString("2026-09-27T0%1:31:00Z").arg(round); moved["message_count"]=6+round; moved["revision"]=4+round;
            h.rows.prepend(moved);
            h.backend.refreshState(true,false,true); settle(400); check(QString("reply moved it up, round %1").arg(round));
        }
        qInfo()<<"A refreshed row that moves is shown once.";
    }
    static void danglingViewReference() {
        Harness h;
        h.missingMember="gone-member";
        h.backend.remember("readerY",12); h.backend.remember("selectedMember","gone-member");
        h.backend.saveView(h.backend.m_account);
        await([&]{return h.viewSaves.size()>=2 && h.backend.m_viewSaving.isEmpty();},"view saves again without the removed message");
        require(h.viewSaves[0]["view"].toObject()["selectedMember"]=="gone-member" && !h.viewSaves.last()["view"].toObject().contains("selectedMember")
            && h.viewSaves.last()["view"].toObject()["readerY"]==12,"only the removed reference leaves the saved view");
        require(h.backend.error().isEmpty() && h.backend.m_viewPending.value(h.backend.m_account).isEmpty(),"the rest of the view saves without an error");
    }
    // Mail is local: a dropped engine is started again without a click, and
    // closing without one always works. An unsaved reply asks once first.
    static void engineDrop() {
        Harness h;
        h.backend.m_startupClock.start();
        h.backend.remember("readerY",40);
        h.backend.m_socket.abort();
        require(!h.backend.m_connected && !h.backend.m_viewDirty.isEmpty(),"the engine is gone with the view unsaved");
        await([&]{return h.backend.m_launchAttempts>=1;},"the engine is started again");
        bool closed=false;
        QObject::connect(&h.backend,&MailBackend::shutdownReady,[&]{closed=true;});
        h.backend.m_draftDirty=true;
        h.backend.shutdown();
        require(!closed && h.backend.error().contains("close again"),"an unsaved reply asks before closing");
        h.backend.shutdown();
        require(closed,"closing again quits without the engine");
    }
    static void memoryChecks(){readerMemorySharing();controls();}
    static void refreshChecks(){scopedRefreshes();mutations();undoActions();remoteMail();reviewTransitions();}
    // Done is This Week, This Month, then older months. Older months start
    // folded and build no rows; each choice is remembered with the view, and
    // the week follows its setting.
    static void doneGroups() {
        Harness h;
        h.filterCollapsed = true;
        h.backend.m_openSections.clear();
        const auto make=[](const QString &id,const QString &group,const QString &at){
            return QVariant(QVariantMap{{"id",id},{"subject","Thread "+id},{"sender_name","Sender"},{"sender_email","sender@example.test"},{"latest_at",at},
                {"message_ids",QVariantList{id+"-original"}},{"revision",3},{"message_count",1},{"categories",QVariantList{}},
                {"group",group},{"kind","people"},{"unread",group=="review"},{"starred",false}}); };
        h.rows = {make("r","review","2026-10-14T12:00:00Z"), make("w","done","2026-10-13T12:00:00Z"), make("m","done","2026-10-02T12:00:00Z"),
            make("s","done","2026-09-10T12:00:00Z"), make("o","done","2025-12-01T12:00:00Z")};
        h.summary={{"filtered_group_counts",QJsonObject{{"review",1},{"held",0},{"done",4}}},{"done_months",QJsonArray{
            QJsonObject{{"month","week"},{"count",1}},QJsonObject{{"month","month"},{"count",1}},QJsonObject{{"month","2026-09"},{"count",1}},QJsonObject{{"month","2025-12"},{"count",1}}}}};
        const auto ids=[&]{ QStringList out; for (const auto &row:h.backend.conversations()) out.append(row.toMap()["id"].toString()); return out; };
        const auto sections=[&]{ QStringList out; for (const auto &row:h.backend.conversations()) out.append(row.toMap()["section"].toString()); return out; };
        h.backend.m_folder="inbox"; h.backend.m_selected.clear(); h.backend.loadConversations();
        await([&]{return !h.backend.m_loading;},"the Inbox loads");
        require(h.lastQuery["done_week_start"]=="monday" && h.lastQuery["today"]=="2026-10-14" && h.lastQuery["open_months"].toArray()==QJsonArray{},"the list asks for Monday weeks with every older month folded");
        require(ids()==QStringList{"r","w","m"} && sections()==QStringList{"review","done:week","done:month"},"This Week and This Month show; older months build no rows");
        require(!h.backend.sectionCollapsed("done:week") && !h.backend.sectionCollapsed("done:month") && h.backend.sectionCollapsed("done:2026-09") && h.backend.sectionCollapsed("done:undated"),"older months start folded");
        h.backend.setSectionCollapsed("done:2026-09",false);
        await([&]{return !h.backend.m_loading && ids().contains("s");},"opening a month loads its rows");
        require(ids()==QStringList{"r","w","m","s"} && h.lastQuery["open_months"].toArray()==QJsonArray{"2026-09"},"only the opened month joins the list");
        require(h.backend.m_views["personal"].toMap()["openSections"].toStringList()==QStringList{"done:2026-09"},"the opened month is remembered");
        h.backend.setSectionCollapsed("done:week",true);
        await([&]{return !h.backend.m_loading && !ids().contains("w");},"folding This Week");
        require(h.backend.m_views["personal"].toMap()["collapsedSections"].toStringList()==QStringList{"done:week"} && h.lastQuery["collapsed_months"].toArray()==QJsonArray{"week"},"a folded This Week is remembered");
        h.backend.setSectionCollapsed("done:week",false);
        await([&]{return !h.backend.m_loading && ids().contains("w");},"This Week opens again");
        // A Wednesday week starts today, so Tuesday's thread is This Month.
        h.backend.m_preferences["mail_week_start"]="wednesday"; h.backend.refreshDoneSections();
        await([&]{return !h.backend.m_loading && h.lastQuery["done_week_start"]=="wednesday";},"the week start reaches the list");
        require(sections()==QStringList{"review","done:month","done:month","done:2026-09"},"rows regroup by the chosen weekday");
    }
    static void run(){scopedRefreshes();readerMemorySharing();photoFillsCircle();forwardLayout();pagingKeepsLength();refreshedRowMoves();replyAfterDiscard();mutations();danglingViewReference();undoActions();remoteMail();controls();reviewTransitions();monthToggles();monthFoldFrames();listGeometry();searchOpensMonths();doneGroups();engineDrop();accountControls();qInfo()<<"Inbox scope, immutable bulk targets, native navigation/layout and account backlog controls pass.";}
};
int main(int argc,char **argv){QGuiApplication app(argc,argv);QQuickStyle::setStyle("Basic");for(const auto *font:{"Geist-Regular.ttf","Geist-Medium.ttf","Geist-SemiBold.ttf"}) require(QFontDatabase::addApplicationFont(QString(FM_GUI_SOURCE_DIR)+"/fonts/"+font)>=0,"bundled font loads");QGuiApplication::setFont(QFont("Geist",10));if(app.arguments().contains("--gpu")) { MailSelectionTest::photoFillsCircle(true); MailSelectionTest::folderSwitchSpeed(); MailSelectionTest::monthFoldFrames(); MailSelectionTest::listGeometry(); } else if(app.arguments().contains("--memory-check")) MailSelectionTest::memoryChecks(); else if(app.arguments().contains("--refresh-check")) MailSelectionTest::refreshChecks(); else MailSelectionTest::run();return 0;}
