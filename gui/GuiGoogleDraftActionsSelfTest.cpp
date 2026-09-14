#include "GuiGoogleDraftActionsSelfTest.h"
#include "GuiGoogleDraftSnapshotSelfTest.h"
#include "MailBackend.h"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QMouseEvent>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSaveFile>
namespace {
const QStringList quotaKeys{"gmail_project_units_per_minute", "gmail_account_units_per_minute",
    "calendar_project_units_per_minute", "calendar_account_units_per_minute"};
QQuickItem *named(QQuickItem *root,const QString &name) {
    if (!root) return nullptr;
    if (root->objectName()==name) return root;
    for (auto child:root->childItems()) if(auto found=named(child,name)) return found;
    return nullptr;
}
QString hashFile(const QString &path) {
    QFile file(path); if(!file.open(QIODevice::ReadOnly)||file.size()>35*1024*1024) return {};
    QCryptographicHash hash(QCryptographicHash::Sha256);
    return hash.addData(&file)?QString::fromLatin1(hash.result().toHex()):QString{};
}
}
GuiGoogleDraftActionsSelfTest::GuiGoogleDraftActionsSelfTest(MailBackend &backend,QQuickWindow *window,
        const QString &expectation,const QString &report,QObject *parent)
    :QObject(parent),m_backend(backend),m_window(window),m_report(report) {
    if(QFileInfo::exists(report)) { m_finished=true; qCritical()<<"Refusing native action report overwrite"<<report;
        QTimer::singleShot(0,this,[]{QCoreApplication::exit(5);}); return; }
    QFile input(expectation);
    if(!input.open(QIODevice::ReadOnly)||input.size()>1024*1024) {
        QTimer::singleShot(0,this,[this]{finish("Missing or oversized native action expectation.");}); return; }
    const auto object=QJsonDocument::fromJson(input.readAll()).object(); m_expected=object["accounts"].toArray();
    if(object["version"]!=1||m_expected.isEmpty()||m_expected.size()>4||!window) {
        QTimer::singleShot(0,this,[this]{finish("Invalid action expectation or window.");}); return; }
    m_limits=object["limits"].toObject();
    if(m_limits.size()!=quotaKeys.size()) {
        QTimer::singleShot(0,this,[this]{finish("Native actions require the four independently supplied sync limits.");}); return; }
    for(const auto &key:quotaKeys) {
        const auto value=m_limits[key].toInteger(-1);
        if(value<100||value>100000000||m_limits[key].toDouble()!=double(value)) {
            QTimer::singleShot(0,this,[this,key]{finish("Invalid expected sync limit: "+key);}); return; }
    }
    for(const auto &value:m_expected) {
        const auto account=value.toObject();
        for(const auto action:{"copy","adopt","delete"}) {
            const auto row=account[action].toObject();
            if(account["account"].toString().isEmpty()||row["provider_draft_id"].toString().isEmpty()
                ||row["source_revision"].toInteger()<=0||row["source_sha256"].toString().size()!=64) {
                QTimer::singleShot(0,this,[this]{finish("Incomplete account/source action expectation.");}); return; }
        }
    }
    window->setWidth(1280); window->setHeight(900); window->setProperty("workspace","mail");
    window->setProperty("settingsOpen",false);
    m_timer.setInterval(50); connect(&m_timer,&QTimer::timeout,this,&GuiGoogleDraftActionsSelfTest::tick);
    m_watchdog.setSingleShot(true); connect(&m_watchdog,&QTimer::timeout,this,[this]{
        finish(QString("Native actions timed out: stage %1 account %2 action %3 draft %4; %5; %6")
            .arg(m_stage).arg(m_account).arg(m_action).arg(m_backend.activeDraftId(),
                m_backend.draftAuthoring().value("status").toString(),m_backend.googleDraft().value("status").toString()));});
    m_watchdog.start(120000); m_timer.start();
}
bool GuiGoogleDraftActionsSelfTest::click(const QString &name) {
    if(!m_window) return false;
    auto item=named(m_window->contentItem(),name);
    if(!item||!item->isVisible()||!item->isEnabled()||item->width()<=0||item->height()<=0) return false;
    for(auto ancestor=item->parentItem();ancestor;ancestor=ancestor->parentItem()) {
        if(!ancestor->property("contentY").isValid()) continue;
        const auto p=item->mapToItem(ancestor,{item->width()/2,item->height()/2});
        if(p.y()>=8&&p.y()<ancestor->height()-8) continue;
        const double origin=ancestor->property("originY").toDouble();
        const double maximum=qMax(origin,origin+ancestor->property("contentHeight").toDouble()-ancestor->height());
        const double next=qBound(origin,ancestor->property("contentY").toDouble()+p.y()-ancestor->height()/2,maximum);
        if(qAbs(next-ancestor->property("contentY").toDouble())>.5){ancestor->setProperty("contentY",next);return false;}
    }
    QPointer<QQuickItem> control=item;
    const auto p=item->mapToScene({item->width()/2,item->height()/2});
    if(!QRectF(0,0,m_window->width(),m_window->height()).contains(p)) return false;
    const QPointF global=m_window->mapToGlobal(p.toPoint());
    QMouseEvent move(QEvent::MouseMove,p,global,Qt::NoButton,Qt::NoButton,Qt::NoModifier);
    QCoreApplication::sendEvent(m_window,&move); if(!control) return false;
    QMouseEvent press(QEvent::MouseButtonPress,p,global,Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
    QCoreApplication::sendEvent(m_window,&press);
    QMouseEvent release(QEvent::MouseButtonRelease,p,global,Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
    QCoreApplication::sendEvent(m_window,&release); return true;
}
void GuiGoogleDraftActionsSelfTest::tick() {
    if(m_finished||!m_window) return;
    if(m_loggedStage!=m_stage) {
        m_loggedStage=m_stage;
        qInfo()<<"Native draft actions stage"<<m_stage<<"account index"<<m_account<<"action index"<<m_action;
    }
    if(!m_backend.error().isEmpty()){finish(m_backend.error());return;}
    const auto account=m_expected[m_account].toObject(); const auto id=account["account"].toString();
    const auto action=QStringList{"copy","adopt","delete"}.value(m_action);
    const auto expected=account[action].toObject(); QString error;
    switch(m_stage) {
    case 0:
        if (const auto loader = named(m_window->contentItem(), "startupOverlay"); loader && loader->isVisible()) return;
        if(!m_backend.connected()||m_backend.accounts().isEmpty())return;
        if(m_backend.syntheticProfile()){finish("Actions require the isolated live-mode fixture.");return;}
        m_stage=1; break;
    case 1: if(click("accountTab_"+id))m_stage=2; break;
    case 2:
        if(m_backend.account()!=id||m_backend.loading()||m_backend.draftBusy())return;
        if(m_backend.googleSyncState(id).value("owned").toBool()){finish("Unexpected native owner before explicit Settings start.");return;}
        if(click("settingsButton"))m_stage=3; break;
    case 3: if(click("settingsPage_Accounts"))m_stage=4; break;
    case 4:
        if(click("googleSync_"+id)){m_quotaFilled=false;m_quotaSubmitted=false;m_stage=5;} break;
    case 5: {
        if(!m_backend.googleSyncState(id).value("owned").toBool()) {
            auto dialog=m_window->findChild<QObject*>("googleSyncLimitsDialog");
            if(!dialog||!dialog->property("visible").toBool()||m_quotaSubmitted)return;
            if(!m_quotaFilled) {
                for(const auto &key:quotaKeys) {
                    auto field=named(m_window->contentItem(),key);
                    if(!field||!field->isVisible()||!field->isEnabled())return;
                }
                for(const auto &key:quotaKeys) {
                    auto field=named(m_window->contentItem(),key);
                    const auto text=QString::number(m_limits[key].toInteger());
                    field->forceActiveFocus();
                    if(!field->setProperty("text",text)||!QMetaObject::invokeMethod(field,"textEdited")
                        ||field->property("text").toString()!=text) {
                        finish("Native quota field edit failed: "+key);return;
                    }
                }
                m_quotaFilled=true;
                qInfo()<<"Native draft actions filled all four Settings quota fields.";
                return;
            }
            if(click("saveGoogleSyncLimits")) {
                m_quotaSubmitted=true;
                qInfo()<<"Native draft actions submitted Settings Start sync.";
            }
            return;
        }
        const auto limits=QJsonObject::fromVariantMap(m_backend.googleSyncLimits());
        if(limits!=m_limits){finish("Native owned sync limits differ from the independent expectation.");return;}
        m_accountResult={{"account",id},{"limits",limits}};m_exports={};m_action=0;
        if(click("settingsDoneButton"))m_stage=6; break;
    }
    case 6:
        if(!m_backend.query().isEmpty()){m_backend.setQuery({});return;}
        if(m_backend.folder()=="drafts"||click("draftsScopeButton")){m_pages=0;m_stage=7;} break;
    case 7: {
        if(m_backend.folder()!="drafts"||m_backend.loading()||m_backend.draftBusy())return;
        int index=0;m_sourceId.clear();
        for(const auto &entry:m_backend.conversations()) {
            const auto row=entry.toMap();
            if(row.value("read_only").toBool()&&row.value("remote_source").toMap().value("provider_draft_id").toString()==expected["provider_draft_id"].toString()) {
                m_sourceId=row.value("draft_id").toString();break;
            }
            ++index;
        }
        auto list=named(m_window->contentItem(),"conversationList");if(!list)return;
        if(m_sourceId.isEmpty()) {
            if(!m_backend.hasMore()||m_pages>=10){finish("Expected exact remote source is absent from bounded Drafts pages.");return;}
            QMetaObject::invokeMethod(list,"positionViewAtEnd");if(click("loadMoreConversations"))++m_pages;return;
        }
        QMetaObject::invokeMethod(list,"positionViewAtIndex",Q_ARG(int,index),Q_ARG(int,3));m_stage=8;break;
    }
    case 8: if(click("conversationRow_"+m_sourceId))m_stage=9;break;
    case 9: {
        if(m_backend.activeDraftId()!=m_sourceId||m_backend.draftBusy())return;
        const auto source=m_backend.draft().value("remote_source").toMap();
        if(source.value("source_revision").toLongLong()!=expected["source_revision"].toInteger()
            ||source.value("sha256").toString()!=expected["source_sha256"].toString()
            ||!GuiGoogleDraftSnapshotSelfTest::verifyReadOnlyControls(m_backend,m_window->contentItem(),error)) {
            finish(error.isEmpty()?"Native source differs from independent expectation.":error);return;
        }
        if(click("draftSourceHistoryButton"))m_stage=10;break;
    }
    case 10:
        if(m_backend.draftSources().value("loading").toBool())return;
        if(click("exportDraftSource_"+QString::number(expected["source_revision"].toInteger())))m_stage=11;break;
    case 11: {
        if(m_backend.draftSources().value("exporting").toBool())return;
        const auto exported=QJsonObject::fromVariantMap(m_backend.draftSources().value("export").toMap());
        if(exported["account"]!=id||exported["draft_id"]!=m_sourceId||exported["source_revision"]!=expected["source_revision"]
            ||!exported["attachment_id"].isNull()||exported["sha256"]!=expected["source_sha256"]
            ||!QFileInfo(exported["path"].toString()).isAbsolute()||hashFile(exported["path"].toString())!=expected["source_sha256"].toString()
            ||QFileInfo(exported["path"].toString()).size()!=exported["size"].toInteger(-1)) {
            finish("Native exported original source failed exact identity/hash/size.");return;
        }
        m_exports.append(exported);if(click("closeDraftSourceHistory"))m_stage=12;break;
    }
    case 12: if(click("draftSourceActionsButton"))m_stage=13;break;
    case 13:
        if(m_backend.draftAuthoring().value("loading").toBool())return;
        if(!m_backend.draftAuthoring().value(action+"_allowed").toBool()) {
            finish("Expected native action refused: "+action+" "+m_backend.draftAuthoring().value(action+"_reason").toString());return;
        }
        if(action=="copy"&&QJsonObject::fromVariantMap(m_backend.draftAuthoring().value("assessment").toMap())["primary_from"]!=expected["primary_from"]){finish("Copy primary From mismatch.");return;}
        if(click(action+"GoogleDraftSource"))m_stage=14;break;
    case 14: {
        auto dialog=m_window->findChild<QObject*>("draftSourceActionDialog");if(!dialog||!dialog->property("visible").toBool())return;
        const auto context=dialog->property("capturedContext").toMap();m_requestId=context.value("request_id").toString();
        if(context.value("account").toString()!=id||context.value("source_revision").toLongLong()!=expected["source_revision"].toInteger()
            ||context.value("source_sha256").toString()!=expected["source_sha256"].toString()||m_requestId.isEmpty()){finish("Confirmation lost exact source identity.");return;}
        if(click("confirmDraftSourceAction"))m_stage=15;break;
    }
    case 15: {
        if(m_backend.draftAuthoring().value("busy").toBool()||m_backend.draftBusy())return;
        if(action=="delete") {
            const auto receipt=QJsonObject::fromVariantMap(m_backend.draftDeletion().value("receipt").toMap());
            if(receipt.isEmpty())return;
            if(receipt["state"]!="deleted"||receipt["request_id"]!=m_requestId||receipt["provider_draft_id"]!=expected["provider_draft_id"]){finish("Native permanent deletion was not exactly confirmed.");return;}
            m_accountResult[action]=QJsonObject{{"request_id",m_requestId},{"state",receipt["state"]},{"revision",receipt["revision"]}};
        } else {
            if(m_backend.activeDraftId()==m_sourceId||m_backend.draftReadOnly())return;
            const auto draft=QJsonObject::fromVariantMap(m_backend.draft());const auto google=draft["google"].toObject();
            if(draft["account"]!=id){finish("Action opened a draft in another account.");return;}
            if(action=="copy") {
                const auto subset=expected["expected_draft"].toObject();
                for(auto it=subset.begin();it!=subset.end();++it) if(draft[it.key()]!=it.value()){finish("Copied draft differs at "+it.key());return;}
                if(google["state"]!="synced")return;
                if(google["baseline"].toObject()["draft_id"].toString().isEmpty()){finish("Copy sync lacks confirmed target.");return;}
            } else if(google["state"]!="adopted"||!google["baseline"].isNull()||!google["operation"].isNull()) {
                finish("Adoption uploaded or fabricated an ACK/baseline without an edit.");return;
            }
            m_accountResult[action]=QJsonObject{{"draft_id",draft["draft_id"]},{"request_id",m_requestId}};
        }
        if(++m_action<3){m_stage=7;break;}
        m_accountResult["exports"]=m_exports;m_observed.append(m_accountResult);m_stage=16;break;
    }
    case 16: if(click("settingsButton"))m_stage=17;break;
    case 17: if(click("settingsPage_Accounts"))m_stage=18;break;
    case 18: if(click("googleSync_"+id))m_stage=19;break;
    case 19:
        if(m_backend.googleSyncState(id).value("owned").toBool()||m_backend.googleSyncState(id).value("changing").toBool())return;
        if(click("settingsDoneButton")) { if(++m_account>=m_expected.size()){finish();return;} m_action=0;m_stage=1; } break;
    }
}
void GuiGoogleDraftActionsSelfTest::finish(const QString &error) {
    if(m_finished)return;m_finished=true;m_timer.stop();m_watchdog.stop();
    QJsonObject result{{"version",1},{"result",error.isEmpty()?"passed":"failed"},{"error",error},{"stage",m_stage},
        {"gui_pid",QCoreApplication::applicationPid()},{"accounts",m_observed}};
    QSaveFile file(m_report);
    if(QFileInfo::exists(m_report)||!file.open(QIODevice::WriteOnly)||file.write(QJsonDocument(result).toJson())<0||!file.commit()){
        qCritical()<<"Cannot retain native action report"<<m_report;QCoreApplication::exit(5);return;}
    m_passed=error.isEmpty();if(!m_passed){qCritical().noquote()<<error;QCoreApplication::exit(5);return;}
    qInfo()<<"Native explicit copy/adopt/delete, exact source exports and Settings owner shutdown passed.";
    m_backend.setProperty("selfTestCompleted",true);m_backend.shutdown();
}
