#include "GoogleOperationTestFixture.h"
#include "GuiGoogleDraftSnapshotSelfTest.h"
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
QQuickItem *named(QQuickItem *item, const QString &name) {
    if (item->objectName() == name) return item;
    for (auto child : item->childItems()) if (auto found = named(child, name)) return found;
    return nullptr;
}
QJsonObject imported(qint64 sourceRevision = 3) {
    return {{"account", "fixture-account"}, {"draft_id", "remote-draft"}, {"origin", "google_snapshot"}, {"read_only", true},
        {"conversation_id", QJsonValue::Null}, {"mode", "new"}, {"revision", 1}, {"discarded", false},
        {"from", "alias@example.test"}, {"from_name", "Exact remote author"}, {"subject", "Remote source 日本語"},
        {"body", QString("Preserved text café source %1").arg(sourceRevision)}, {"body_html", "<p>Different HTML<img src=\"https://never-fetch.example/image\"></p>"},
        {"signature_text", ""}, {"attachment_ids", QJsonArray{"retained-part"}},
        {"attachments", QJsonArray{QJsonObject{{"account", "fixture-account"}, {"id", "retained-part"}, {"name", "binary.bin"}, {"mime_type", "application/octet-stream"}, {"size", 3}, {"state", "available"}}}},
        {"to_recipients", QJsonArray{QJsonObject{{"name", "Exact To"}, {"email", "to@example.test"}}}},
        {"cc_recipients", QJsonArray{QJsonObject{{"name", "Exact Cc"}, {"email", "cc@example.test"}}}},
        {"bcc_recipients", QJsonArray{QJsonObject{{"name", "Exact Bcc"}, {"email", "bcc@example.test"}}}},
        {"google", QJsonValue::Null}, {"google_send", QJsonValue::Null}, {"google_delete", QJsonValue::Null},
        {"remote_source", QJsonObject{{"provider_draft_id", "rd0000"}, {"source_revision", sourceRevision},
            {"source_state", "available"}, {"preview_state", "partial"}, {"sha256", QString(64,'a')},
            {"seen_generation",2},{"last_observed_at",1700000000},{"latest_scan_generation",2},
            {"seen_in_latest_scan",true},{"latest_scan_complete",true},{"absence_means_sent",false},{"attachment_ids",QJsonArray{"retained-part"}},
            {"issues", QJsonArray{"unsupported_from", "HTML source retained"}},
            {"raw_headers", QJsonObject{{"from", "Exact remote author <alias@example.test>"}, {"to", "Team: Exact To <to@example.test>;"},
                {"cc", "Exact Cc <cc@example.test>"}, {"bcc", "Exact Bcc <bcc@example.test>"}, {"reply_to", "Different reply <reply@example.test>"}}}}}};
}
}

class GoogleDraftSnapshotControlsTest {
public:
    static void run() {
        for (const auto font : {"Geist-Regular.ttf", "Geist-Medium.ttf", "Geist-SemiBold.ttf"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/" + font) >= 0, "actual bundled Geist font loads");
        Engine fixture;
        MailBackend backend(fixture.profile, "unused-fixture-engine", nullptr, false, true);
        backend.m_connected = true; backend.m_account = "fixture-account";
        backend.m_activeDraft = "remote-draft"; backend.m_generation = 1;
        backend.m_enginePid = QCoreApplication::applicationPid(); backend.m_draft = imported().toVariantMap();
        backend.m_accounts = {QVariantMap{{"id","fixture-account"}, {"name","Must not replace From"},
            {"google",QVariantMap{{"capabilities",QVariantMap{{"mail_read",true},{"mail_delivery",true},{"send",true}}}}}}};
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(fixture.server.serverName());
        await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached native socket connects");
        bool hello = false;
        backend.request("hello", {}, {{"token","synthetic-operation-token"}}, [&](auto result, bool ok) { hello = ok && result["mode"] == "live"; });
        await([&] { return hello; }, "cached native handshake");
        QPointer<QLocalSocket> heldPeer; QString heldId; QJsonObject heldRequest;
        QJsonArray historySources{imported(4)["remote_source"]};
        fixture.customRequest = [&](QLocalSocket *peer, const QJsonObject &request) {
            const auto method = request["method"].toString();
            if (method == "draft_get") { heldPeer=peer; heldId=request["id"].toString(); heldRequest=request; return true; }
            if (method == "google_draft_sources") {
                peer->write(reply(request["id"].toString(), {{"account","fixture-account"},{"draft_id","remote-draft"},
                    {"sources",historySources},{"total",historySources.size()},{"next_offset",QJsonValue::Null}}));
                return true;
            }
            return false;
        };
        QTemporaryDir copy; require(copy.isValid(), "private copied production QML");
        const auto modulePath=copy.filePath("DraftTest"); require(QDir().mkpath(modulePath),"QML module directory");
        for(const auto name:{"Theme.qml","FmDialog.qml", "FmConfirmDialog.qml", "FmButton.qml","FmToolTip.qml","FmIcon.qml","RecipientField.qml","ReplyComposer.qml"})
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR)+'/'+name,modulePath+'/'+name),"copy actual composer source");
        QFile module(modulePath+"/qmldir"); require(module.open(QIODevice::WriteOnly),"module metadata");
        module.write("module DraftTest\nsingleton Theme 1.0 Theme.qml\nReplyComposer 1.0 ReplyComposer.qml\nFmButton 1.0 FmButton.qml\nFmToolTip 1.0 FmToolTip.qml\nFmIcon 1.0 FmIcon.qml\nRecipientField 1.0 RecipientField.qml\n"); module.close();
        qmlRegisterType<ReplyDocument>("DraftTest",1,0,"ReplyDocument");
        QQmlApplicationEngine qml; bool qmlError=false;
        QObject::connect(&qml,&QQmlEngine::warnings,[&](const QList<QQmlError>&errors){ qmlError=true; for(const auto &error:errors) qWarning()<<error.toString(); });
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend",&backend);
        qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import DraftTest 1.0
ApplicationWindow {
    width: 390; height: 1500; visible: true; color: Theme.reader
    function appearance(theme, size) { Theme.themeId=theme; Theme.textSize=size }
    ReplyComposer { objectName: "composer"; width: parent.width }
})QML", QUrl::fromLocalFile(copy.filePath("Harness.qml")));
        require(!qml.rootObjects().isEmpty(),"actual imported composer loads");
        auto window=qobject_cast<QQuickWindow*>(qml.rootObjects().first()); require(window,"native window");
        QString error;
        for(const auto theme:{"graphite","paper","harbour"}) for(const auto size:{"default","large"}) {
            require(QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(size))),"actual theme/size bindings");
            QCoreApplication::processEvents();
            const bool controlsValid = GuiGoogleDraftSnapshotSelfTest::verifyReadOnlyControls(backend,window->contentItem(),error);
            require(controlsValid,qPrintable(error));
            for(const auto label:{"From","To","Cc","Bcc","Reply-To"}) {
                auto field=named(window->contentItem(),QString("draftSnapshotHeader_")+label);
                require(field && field->property("readOnly").toBool() && field->width()>100 && field->width()<=window->width(),"bounded selectable complete routing header");
                require(!field->property("text").toString().isEmpty(),"retained raw routing field displayed");
            }
            require(named(window->contentItem(),"draftSnapshotHeader_From")->property("text").toString()=="Exact remote author <alias@example.test>","From never falls back to account identity");
            require(named(window->contentItem(),"draftSnapshotHeader_To")->property("text").toString()=="Exact To <to@example.test>","all parsed routing recipients remain intact in selectable preview");
            const auto captures = qEnvironmentVariable("FM_GUI_SNAPSHOT_SCREENSHOTS");
            if (!captures.isEmpty()) {
                require(QDir().mkpath(captures), "designated screenshot directory");
                const auto path = QDir(captures).filePath(QString("snapshot-%1-%2.png").arg(theme,size));
                require(!QFile::exists(path), "never overwrite existing native screenshot");
                const auto image = window->grabWindow();
                require(!image.isNull() && image.save(path), "actual narrow rendered preview screenshot");
                qInfo() << "Snapshot geometry" << theme << size << window->size() << window->devicePixelRatio() << image.size();
            }
        }
        require(fixture.operations==0 && !backend.m_draftDirty,"read-only selection and blocked mutation paths dispatch no operation");
        backend.m_draftInventoryAccount="fixture-account";
        backend.m_draftInventory={{"generation",3},{"observed_this_scan",2},{"observed_items",99},{"pending_items",1},{"enumeration_complete",false}};
        backend.m_googleSync={{"fixture-account",QVariantMap{{"active",true},{"progress",QVariantMap{{"remote_drafts","running"}}}}}};
        require(backend.googleDraftInventory().value("status").toString()=="Checking Google drafts · 2 observed · 1 pending",
            "active scan label uses this scan's observations rather than retained lifetime count");
        backend.m_googleSync.clear(); backend.m_draftInventory.clear(); backend.m_draftInventoryAccount.clear();

        backend.loadActiveDraft(); await([&]{return !heldId.isEmpty();},"hold source revision 3 cached reply");
        backend.m_draft=imported(4).toVariantMap(); emit backend.draftChanged();
        heldPeer->write(reply(heldId,imported(3)));
        await([&]{return backend.m_pending.isEmpty();},"deliver old source reply");
        require(backend.m_draft==imported(4).toVariantMap(),"source revision floor preserves newer observed source even at same local revision");
        heldId.clear(); backend.loadActiveDraft(); await([&]{return !heldId.isEmpty();},"hold empty-scan freshness update");
        auto unseen = imported(4); auto freshness = unseen["remote_source"].toObject();
        freshness["latest_scan_generation"]=3; freshness["seen_in_latest_scan"]=false; unseen["remote_source"]=freshness;
        heldPeer->write(reply(heldId,unseen)); await([&]{return backend.m_pending.isEmpty();},"completed empty scan retains snapshot");
        require(backend.draft().value("body")==imported(4)["body"].toVariant()
            && named(window->contentItem(),"draftSnapshotNotice")->property("text").toString().contains("Not seen in the latest completed check")
            && backend.draft().value("remote_source").toMap().value("absence_means_sent")==false,
            "native freshness explicitly retains unseen cached source without inferring sending or deletion");
        heldId.clear(); backend.loadActiveDraft(); await([&]{return !heldId.isEmpty();},"hold older same-source scan metadata");
        heldPeer->write(reply(heldId,imported(4))); await([&]{return backend.m_pending.isEmpty();},"late older scan metadata returned");
        require(QJsonObject::fromVariantMap(backend.m_draft)==unseen,"same-source older scan metadata cannot erase unseen freshness");
        heldId.clear(); backend.loadActiveDraft(); await([&]{return !heldId.isEmpty();},"hold prior account source reply");
        backend.m_account="other-account"; backend.m_activeDraft="other-draft"; ++backend.m_generation;
        auto other=imported(8); other["account"]="other-account"; other["draft_id"]="other-draft";
        backend.m_draft=other.toVariantMap(); emit backend.accountChanged(); emit backend.draftChanged();
        heldPeer->write(reply(heldId,imported(5))); await([&]{return backend.m_pending.isEmpty();},"deliver prior account response");
        require(backend.m_draft==other.toVariantMap(),"late source cannot replace another account's draft");
        backend.m_account="fixture-account"; backend.m_activeDraft="remote-draft"; ++backend.m_generation;
        auto missing=imported(4); auto unavailable=imported(5)["remote_source"].toObject();
        unavailable["source_state"]="missing"; unavailable["preview_state"]="unavailable";
        unavailable["sha256"]=QJsonValue::Null; unavailable["attachment_ids"]=QJsonArray{}; unavailable["raw_headers"]=QJsonObject{};
        missing["remote_source"]=unavailable;
        backend.m_draft=missing.toVariantMap(); emit backend.accountChanged(); emit backend.draftChanged();
        require(named(window->contentItem(),"draftSnapshotNotice")->property("text").toString().contains("Earlier cached preview")
            && !named(window->contentItem(),"exportDraftAttachment_retained-part")->isEnabled(),
            "missing newest source labels earlier cached content and disables current-version attachment export");
        backend.exportDraftSource(5,"retained-part");
        require(fixture.operations==0,"unavailable current source refuses direct export before auxiliary dispatch");
        auto history=named(window->contentItem(),"draftSourceHistoryButton");
        require(history && QMetaObject::invokeMethod(history,"clicked"),"actual saved source history handler");
        await([&]{return backend.draftSources().value("sources").toList().size()==1;},"cached source history returns exact version");
        fixture.expectedMethod="google_draft_source";
        fixture.expectedParams={{"draft_id","remote-draft"},{"source_revision",4},{"attachment_id","retained-part"}};
        auto oldFile=named(window->contentItem(),"exportHistoricalDraftAttachment_4_retained-part");
        require(oldFile && QMetaObject::invokeMethod(oldFile,"clicked"),"actual historical retained-file export handler");
        await([&]{return fixture.operations==1;},"historical file uses exact old revision");
        fixture.complete({{"account","fixture-account"},{"draft_id","remote-draft"},{"source_revision",4},{"attachment_id","retained-part"},
            {"sha256",QString(64,'b')},{"size",3},{"path",QDir(fixture.profile).absoluteFilePath("binary.bin")},{"name","binary.bin"},{"mime_type","application/octet-stream"}});
        await([&]{return backend.m_googleOperations.isEmpty();},"historical file export delivered");
        require(backend.draftSources().value("export").toMap().value("source_revision").toLongLong()==4
            && backend.m_draft==missing.toVariantMap(),"historical file copy leaves the current missing-source preview unchanged");
        fixture.expectedParams={{"draft_id","remote-draft"},{"source_revision",4}};
        auto exportButton=named(window->contentItem(),"exportDraftSource_4");
        require(exportButton && QMetaObject::invokeMethod(exportButton,"clicked"),"actual revision-bound export handler");
        await([&]{return fixture.operations==2;},"source export auxiliary request");
        backend.cancelGoogleProviderOperations("fixture-account");
        require(backend.m_googleOperations.size()==1,"pausing Google does not cancel local verified source export");
        backend.m_account="other-account"; backend.m_activeDraft="other-draft"; backend.m_draft=other.toVariantMap();
        emit backend.accountChanged(); emit backend.draftChanged();
        require(backend.draftSources().value("export").toMap().isEmpty()
            && !backend.draftSources().value("exporting").toBool(),"account switch clears prior source export state");
        const QPointer<QLocalSocket> olderExportPeer=fixture.operationPeer;
        backend.m_account="fixture-account"; backend.m_activeDraft="remote-draft"; backend.m_draft=missing.toVariantMap();
        emit backend.accountChanged(); emit backend.draftChanged();
        fixture.expectedParams={{"draft_id","remote-draft"},{"source_revision",4},{"attachment_id","retained-part"}};
        backend.exportDraftSource(4,"retained-part");
        await([&]{return fixture.operations==3;},"new export after A to B to A navigation");
        require(olderExportPeer && backend.m_googleOperations.size()==2,"old and new exports are independently in flight");
        olderExportPeer->write(reply("operation",{{"account","fixture-account"},{"draft_id","remote-draft"},{"source_revision",4},{"attachment_id",QJsonValue::Null},
            {"sha256",QString(64,'a')},{"size",5},{"path",QDir(fixture.profile).absoluteFilePath("export.eml")},{"name","export.eml"},{"mime_type","message/rfc822"}}));
        await([&]{return backend.m_googleOperations.size()==1;},"deliver old raw export after new attachment export started");
        require(backend.draftSources().value("exporting").toBool()
            && backend.draftSources().value("export").toMap().isEmpty()
            && backend.m_draft==missing.toVariantMap(),"old same-key reply cannot clear the new busy state or publish its raw copy");
        backend.loadDraftSources();
        await([&]{return !backend.draftSources().value("loading").toBool();},"history read while current export is pending");
        fixture.complete({{"account","fixture-account"},{"draft_id","remote-draft"},{"source_revision",4},{"attachment_id","retained-part"},
            {"sha256",QString(64,'b')},{"size",3},{"path",QDir(fixture.profile).absoluteFilePath("binary.bin")},{"name","binary.bin"},{"mime_type","application/octet-stream"}});
        await([&]{return backend.m_googleOperations.isEmpty();},"new export completed independently of history generation");
        require(!backend.draftSources().value("exporting").toBool()
            && backend.draftSources().value("export").toMap().value("attachment_id")=="retained-part"
            && backend.draftSources().value("export").toMap().value("sha256")==QString(64,'b')
            && backend.m_draft==missing.toVariantMap(),"new same-key attachment result is retained without changing the current preview");
        backend.m_account="other-account"; backend.m_activeDraft="other-draft"; backend.m_draft=other.toVariantMap();
        emit backend.accountChanged(); emit backend.draftChanged();

        // Rendering fixture: genuine source states, a long numeric revision and
        // the retained Unicode filename returned by the ordinary export handler.
        constexpr qint64 longRevision=1234567890123;
        const QString longName="Quarterly agreement – architecture, original documents and 日本語 supporting material – reviewed September 2026.bin";
        auto longDraft=imported(longRevision);
        auto longAttachment=longDraft["attachments"].toArray().first().toObject();
        longAttachment["name"]=longName; longDraft["attachments"]=QJsonArray{longAttachment};
        historySources=QJsonArray{longDraft["remote_source"]};
        backend.m_account="fixture-account"; backend.m_activeDraft="remote-draft"; backend.m_draft=longDraft.toVariantMap();
        emit backend.accountChanged(); emit backend.draftChanged();
        history=named(window->contentItem(),"draftSourceHistoryButton");
        require(history && QMetaObject::invokeMethod(history,"clicked"),"open actual Saved sources for narrow visual coverage");
        auto popup=window->findChild<QObject *>("draftSourceHistoryPopup");
        await([&]{return popup && popup->property("opened").toBool()
            && !backend.draftSources().value("loading").toBool();},"open source popup and loaded history");
        fixture.expectedParams={{"draft_id","remote-draft"},{"source_revision",longRevision},{"attachment_id","retained-part"}};
        auto longFile=named(window->contentItem(),QString("exportHistoricalDraftAttachment_%1_retained-part").arg(longRevision));
        require(longFile && QMetaObject::invokeMethod(longFile,"clicked"),"export long-name historical file through actual handler");
        await([&]{return fixture.operations==4;},"long source file export request");
        fixture.complete({{"account","fixture-account"},{"draft_id","remote-draft"},{"source_revision",longRevision},{"attachment_id","retained-part"},
            {"sha256",QString(64,'b')},{"size",3},{"path",QDir(fixture.profile).absoluteFilePath(longName)},{"name",longName},{"mime_type","application/octet-stream"}});
        await([&]{return backend.m_googleOperations.isEmpty();},"long attachment feedback retained");
        for(const auto theme:{"graphite","paper","harbour"}) for(const auto size:{"default","large"}) {
            require(QMetaObject::invokeMethod(window,"appearance",Q_ARG(QVariant,QString(theme)),Q_ARG(QVariant,QString(size))),"popup theme and font bindings");
            QCoreApplication::processEvents();
            const auto image=window->grabWindow();
            require(!image.isNull(),"open popup actually renders");
            const auto captures=qEnvironmentVariable("FM_GUI_SNAPSHOT_SCREENSHOTS");
            if(!captures.isEmpty()) {
                const auto path=QDir(captures).filePath(QString("snapshot-sources-%1-%2.png").arg(theme,size));
                require(!QFile::exists(path) && image.save(path),"retain fresh open-popup screenshot before geometry assertions");
            }
            auto background=qvariant_cast<QQuickItem *>(popup->property("background"));
            require(background && popup->property("opened").toBool(),"Saved sources remains visibly open");
            const auto popupRect=background->mapRectToScene(QRectF(0,0,background->width(),background->height()));
            require(QRectF(0,0,window->width(),window->height()).contains(popupRect),"popup stays within 390px native window");
            for(const auto &name:QStringList{"closeDraftSourceHistory",QString("exportDraftSource_%1").arg(longRevision),
                    QString("exportHistoricalDraftAttachment_%1_retained-part").arg(longRevision),"showDraftSourceFolder"}) {
                auto button=named(window->contentItem(),name);
                const auto rect=button ? button->mapRectToScene(QRectF(0,0,button->width(),button->height())) : QRectF{};
                require(button && button->isVisible() && button->isEnabled() && !button->property("text").toString().isEmpty()
                    && rect.width()>20 && rect.height()>15 && popupRect.adjusted(.5,.5,-.5,-.5).contains(rect),
                    qPrintable("Source popup button is blank, clipped or outside popup: "+name));
                qInfo()<<"Source popup control"<<theme<<size<<name<<rect;
            }
            auto feedback=named(window->contentItem(),"draftSourceExportStatus");
            require(feedback && feedback->property("text").toString().contains(longName)
                && popupRect.contains(feedback->mapRectToScene(QRectF(0,0,feedback->width(),feedback->height()))),
                "full long Unicode file label remains within source popup");
            qInfo()<<"Source popup geometry"<<theme<<size<<window->size()<<window->devicePixelRatio()<<popupRect;
            require(QMetaObject::invokeMethod(popup,"close"),"close popup for long attachment chip inspection");
            await([&]{return !popup->property("visible").toBool();},"popup closes before long-label preview capture");
            const auto preview=window->grabWindow();
            require(!preview.isNull(),"long-label preview renders");
            if(!captures.isEmpty()) {
                const auto path=QDir(captures).filePath(QString("snapshot-long-file-%1-%2.png").arg(theme,size));
                require(!QFile::exists(path) && preview.save(path),"retain long-file preview before containment assertion");
            }
            auto copyButton=named(window->contentItem(),"exportDraftAttachment_retained-part");
            require(copyButton && copyButton->parentItem() && copyButton->parentItem()->parentItem(),"long-name file chip exists");
            auto chip=copyButton->parentItem()->parentItem();
            const auto chipRect=chip->mapRectToScene(QRectF(0,0,chip->width(),chip->height()));
            const auto copyRect=copyButton->mapRectToScene(QRectF(0,0,copyButton->width(),copyButton->height()));
            qInfo()<<"Long file chip"<<theme<<size<<chipRect<<"copy"<<copyRect;
            require(chipRect.contains(copyRect),"long attachment name pushes Copy outside its chip");
            require(QMetaObject::invokeMethod(history,"clicked"),"reopen source popup through actual control");
            await([&]{return popup->property("opened").toBool() && !backend.draftSources().value("loading").toBool();},"history remains accessible after long-file preview");
        }
        require(QMetaObject::invokeMethod(popup,"close"),"close visual fixture popup");
        backend.m_account="other-account"; backend.m_activeDraft="other-draft"; backend.m_draft=other.toVariantMap();
        emit backend.accountChanged(); emit backend.draftChanged();

        auto local=imported(); local.remove("origin"); local.remove("read_only"); local.remove("remote_source");
        local["account"]="other-account"; local["draft_id"]="local-draft"; local["body_html"]=QJsonValue::Null;
        backend.m_activeDraft="local-draft"; backend.m_draft=local.toVariantMap(); emit backend.draftChanged();
        backend.editDraft("Local authoring still works"); backend.m_draftTimer.stop();
        require(!backend.draftReadOnly() && backend.m_draftDirty && backend.draft().value("body")=="Local authoring still works","local draft authoring remains enabled");
        backend.m_draftDirty=false;
        require(!qmlError,"all three themes and both font sizes have no native QML errors");
        qInfo()<<"Google draft snapshot controls passed: real selectable preview and mutation guards, all themes/narrow fonts, cached source revision/account floors, history/export handlers and provider-Pause isolation. Synthetic local IPC; no provider or latency claim.";
    }
};
int main(int argc,char **argv) {
    QQuickStyle::setStyle("Basic"); QGuiApplication app(argc,argv);
    GoogleDraftSnapshotControlsTest::run(); return 0;
}
