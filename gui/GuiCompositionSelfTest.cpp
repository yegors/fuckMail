#include "GuiCompositionSelfTest.h"
#include "GoogleOperation.h"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDebug>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QInputMethodEvent>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QQuickItem>
#include <QQuickTextDocument>
#include <QRegularExpression>
#include <QSaveFile>
#include <QTextCursor>
#include <QTextDocument>
#include <QUuid>

namespace {
const QString sourceConversation = "windscribe-thread-005";
const QString conflictConversation = "personal-thread-004";
const QString conflictSourceA = "personal-message-004-031";
const QString conflictSourceB = "personal-message-004-000";
const QString frozenContextRefusal = "Not saved — the saved draft has a different original message or composition identity. Your version remains here. Reload the saved draft only to replace these edits.";
const QStringList modes{"new", "reply_all", "forward", "reply"};
QQuickItem *find(QQuickItem *parent, const QString &name) {
    if (parent->isVisible() && parent->objectName() == name) return parent;
    for (auto child : parent->childItems()) if (auto found = find(child, name)) return found;
    return nullptr;
}
QQuickItem *withTip(QQuickItem *parent, const QString &tip) {
    if (parent->isVisible() && parent->property("tip").toString() == tip) return parent;
    for (auto child : parent->childItems()) if (auto found = withTip(child, tip)) return found;
    return nullptr;
}
QQuickItem *withText(QQuickItem *parent, const QString &text) {
    if (parent->isVisible() && parent->property("text").toString()==text) return parent;
    for (auto child:parent->childItems()) if (auto found=withText(child,text)) return found;
    return nullptr;
}
QQuickItem *buttonWithText(QQuickItem *parent, const QString &text) {
    if (parent->isVisible() && parent->property("text").toString()==text
        && parent->metaObject()->indexOfSignal("clicked()")>=0) return parent;
    for (auto child:parent->childItems()) if (auto found=buttonWithText(child,text)) return found;
    return nullptr;
}
void key(QObject *target, int code, Qt::KeyboardModifiers modifiers = Qt::NoModifier) {
    QKeyEvent press(QEvent::KeyPress, code, modifiers), release(QEvent::KeyRelease, code, modifiers);
    QCoreApplication::sendEvent(target, &press); QCoreApplication::sendEvent(target, &release);
}
QString plain(QQuickItem *field) {
    if (!field) return {};
    auto document = qvariant_cast<QQuickTextDocument *>(field->property("textDocument"));
    return document ? document->textDocument()->toPlainText() : field->property("text").toString();
}
bool visibleCenter(QQuickItem *field, QQuickWindow *window) {
    if (!field || !field->isVisible() || field->width() <= 0 || field->height() <= 0) return false;
    const auto center = field->mapToScene({field->width()/2, field->height()/2});
    if (!QRectF(0,0,window->width(),window->height()).contains(center)) return false;
    for (auto ancestor=field->parentItem(); ancestor; ancestor=ancestor->parentItem())
        if (ancestor->clip() && !ancestor->mapRectToScene(QRectF(0,0,ancestor->width(),ancestor->height())).contains(center)) return false;
    return true;
}
void reveal(QQuickItem *field) {
    for (auto ancestor=field->parentItem(); ancestor; ancestor=ancestor->parentItem()) {
        if (!ancestor->property("contentY").isValid() || !ancestor->property("contentHeight").isValid()) continue;
        const auto rect=field->mapRectToItem(ancestor,QRectF(0,0,field->width(),field->height()));
        const auto current=ancestor->property("contentY").toDouble();
        const auto maximum=qMax(0.0,ancestor->property("contentHeight").toDouble()-ancestor->height());
        if (rect.top()<4) ancestor->setProperty("contentY",qBound(0.0,current+rect.top()-4,maximum));
        else if (rect.bottom()>ancestor->height()-4) ancestor->setProperty("contentY",qBound(0.0,current+rect.bottom()-ancestor->height()+4,maximum));
    }
}
QJsonObject durableFields(const QJsonObject &draft) {
    QJsonObject result;
    for (const auto key : {"account","draft_id","conversation_id","mode","source_message_id","source_hash","from","from_name",
        "identity_revision","subject","body","body_html","signature_text","to_recipients","cc_recipients","bcc_recipients",
        "attachment_ids","revision","discarded","reply_to_message_id","references"}) result[key]=draft[key];
    return result;
}
QStringList addresses(const QJsonValue &value) {
    QStringList result; for (const auto &row:value.toArray()) result.append(row.toObject()["email"].toString()); return result;
}
QString cliPath(const QString &engine) {
    return QDir(QFileInfo(engine).absolutePath()).filePath("fm"
#ifdef Q_OS_WIN
        ".exe"
#endif
    );
}
QByteArray header(const QByteArray &bytes,const QByteArray &name) {
    auto rows=bytes.left(bytes.indexOf("\r\n\r\n")).split('\n'); QByteArray result; bool found=false;
    for (auto row:rows) {
        if (row.endsWith('\r')) row.chop(1);
        if (found && (row.startsWith(' ') || row.startsWith('\t'))) { result+=' '; result+=row.trimmed(); }
        else if (row.toLower().startsWith(name.toLower()+':')) { found=true; result=row.mid(name.size()+1).trimmed(); }
        else if (found) break;
    }
    return result;
}
bool verifyMime(const QString &profile,const QJsonObject &draft,const QJsonObject &receipt,QString &error) {
    QFile file(receipt["path"].toString()); const auto path=QFileInfo(file).canonicalFilePath();
    const auto relative=QDir(QFileInfo(profile).canonicalFilePath()).relativeFilePath(path);
    if (!relative.startsWith("exports/") || !relative.endsWith(".eml") || !file.open(QIODevice::ReadOnly) || file.size()>2*1024*1024) {
        error="Composition capture did not retain a bounded export in its profile."; return false;
    }
    const auto bytes=file.readAll();
    if (QString::fromLatin1(QCryptographicHash::hash(bytes,QCryptographicHash::Sha256).toHex())!=receipt["sha256"].toString()
        || receipt["state"]!="accepted" || receipt["network_used"].toBool() || receipt["attempts_started"].toInt()!=1
        || !receipt["draft_unchanged"].toBool() || header(bytes,"Subject")!=draft["subject"].toString().toUtf8()
        || !header(bytes,"From").contains("you@windscribe.example")) {
        error="Composition capture identity, exact subject/From or non-delivery receipt differs."; return false;
    }
    for (const auto field:{"to_recipients","cc_recipients","bcc_recipients"}) {
        const QByteArray name=QByteArray(field).startsWith("to") ? "To" : QByteArray(field).startsWith("cc") ? "Cc" : "Bcc";
        const auto actual=header(bytes,name); const auto expected=addresses(draft[field]);
        // The fixture uses explicit ordinary mailboxes; compare every address,
        // so an unexpected extra recipient cannot pass a substring check.
        static const QRegularExpression mailbox(R"([A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9.-]+)");
        QStringList actualAddresses;
        auto matches=mailbox.globalMatch(QString::fromUtf8(actual));
        while (matches.hasNext()) actualAddresses.append(matches.next().captured());
        if (actualAddresses!=expected || (expected.isEmpty() && !actual.isEmpty())) { error="Capture changed the exact authored recipient list."; return false; }
    }
    const auto reply=header(bytes,"In-Reply-To");
    if (draft["reply_to_message_id"].isNull() ? !reply.isEmpty() : reply!=draft["reply_to_message_id"].toString().toUtf8()) {
        error="Capture changed the frozen reply/forward relationship."; return false;
    }
    const auto part=bytes.indexOf("Content-Type: text/plain");
    const auto begin=part<0 ? -1 : bytes.indexOf("\r\n\r\n",part);
    auto end=begin<0 ? -1 : bytes.indexOf("\r\n--",begin+4);
    if (end<0) end=bytes.size();
    if (begin<0 || !bytes.mid(part,begin-part).contains("Content-Transfer-Encoding: base64")) {
        error="Capture has no verifiable plain MIME alternative."; return false;
    }
    auto encoded=bytes.mid(begin+4,end-begin-4); encoded.replace("\r",""); encoded.replace("\n","");
    const auto decoded=QByteArray::fromBase64(encoded,QByteArray::AbortOnBase64DecodingErrors);
    auto expected=draft["body"].toString(); const auto signature=draft["signature_text"].toString();
    if (!signature.isEmpty()) expected+="\n\n-- \n"+signature;
    if (decoded!=expected.toUtf8()) { error="Capture changed the exact authored body/signature bytes."; return false; }
    return true;
}
}

GuiCompositionSelfTest::GuiCompositionSelfTest(MailBackend &backend,QQuickWindow *window,QString profile,QString engine,
    std::function<void(const QVariantMap &,const QString &)> finished,QObject *parent,bool attachmentsOnly)
    : QObject(parent),m_backend(backend),m_window(window),m_profile(std::move(profile)),m_engine(std::move(engine)),m_finished(std::move(finished)),m_attachmentsOnly(attachmentsOnly) {
    m_timer.setInterval(80); connect(&m_timer,&QTimer::timeout,this,[this] { step(); });
    m_processTimeout.setSingleShot(true); m_processTimeout.setInterval(20000);
    connect(&m_processTimeout,&QTimer::timeout,this,[this] { finish("Composition child/CLI exceeded its bounded wait."); });
    m_timer.start();
}
void GuiCompositionSelfTest::finish(const QString &error) {
    if (!m_finished) return;
    m_timer.stop(); m_processTimeout.stop(); m_cli.disconnect(this); m_child.disconnect(this);
    if (m_cli.state()!=QProcess::NotRunning) m_cli.kill();
    if (m_child.state()!=QProcess::NotRunning) m_child.kill();
    const QVariantMap expected=QJsonObject{{"account","windscribe"},{"drafts",m_drafts},{"captures",m_captures},
        {"attachment_drafts",m_attachmentDrafts},{"attachment_files",m_attachmentFiles},{"attachment_capture",m_attachmentCapture},
        {"restart_draft",m_drafts.isEmpty()?QJsonObject{}:m_drafts.first().toObject()},
        {"source_conversation",sourceConversation},{"source_before",m_sourceBefore}}.toVariantMap();
    auto callback=std::move(m_finished); m_finished={}; deleteLater(); callback(expected,error);
}
QQuickItem *GuiCompositionSelfTest::item(const QString &name) const { return find(m_window->contentItem(),name); }
bool GuiCompositionSelfTest::clickItem(QQuickItem *control) {
    if (!control || !control->isEnabled()) return false;
    if (const auto overlay = item("startupOverlay"); overlay && overlay->isVisible()) return false;
    if (!visibleCenter(control,m_window)) { reveal(control); return false; }
    const auto position=control->mapToScene({control->width()/2,control->height()/2}); const auto global=m_window->mapToGlobal(position.toPoint());
    QMouseEvent press(QEvent::MouseButtonPress,position,global,Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease,position,global,Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
    QCoreApplication::sendEvent(m_window,&press); QCoreApplication::sendEvent(m_window,&release); return true;
}
bool GuiCompositionSelfTest::click(const QString &name) { return clickItem(item(name)); }
bool GuiCompositionSelfTest::type(const QString &name,const QString &text) {
    auto field=item(name); if (!field || !field->isEnabled()) return false;
    reveal(field); field->forceActiveFocus(); QMetaObject::invokeMethod(field,"selectAll");
    if (text.isEmpty()) key(field,Qt::Key_Backspace);
    else { QInputMethodEvent input; input.setCommitString(text); QCoreApplication::sendEvent(field,&input); }
    return plain(field)==text;
}
bool GuiCompositionSelfTest::recipient(const QString &field,const QString &text) {
    const auto name="draftRecipient_"+field;
    if (!type(name,text)) return false; key(item(name),Qt::Key_Return); return true;
}
bool GuiCompositionSelfTest::saved() const {
    return !m_backend.loading() && !m_backend.draftBusy() && !m_backend.draft().isEmpty()
        && (m_backend.draftStatus()=="Saved on this machine" || m_backend.draft()["revision"].toInt()==0);
}
void GuiCompositionSelfTest::cli(const QStringList &arguments,std::function<void(const QJsonObject &)> callback,const QString &account) {
    m_waiting=true; m_cli.disconnect(this);
    connect(&m_cli,&QProcess::finished,this,[this,callback=std::move(callback)](int code,QProcess::ExitStatus status) {
        m_processTimeout.stop(); m_waiting=false;
        const auto response=QJsonDocument::fromJson(m_cli.readAllStandardOutput()).object();
        if (code!=0 || status!=QProcess::NormalExit || !response["ok"].toBool()) { finish("Independent composition CLI failed: "+QString::fromUtf8(m_cli.readAllStandardError())); return; }
        callback(response["result"].toObject());
    });
    connect(&m_cli,&QProcess::errorOccurred,this,[this](auto) { finish("Cannot launch independent composition CLI."); });
    m_processTimeout.start(); m_cli.start(cliPath(m_engine),QStringList{"--profile",m_profile,"--engine",m_engine,"--account",account,"--json"}+arguments);
}
QJsonObject GuiCompositionSelfTest::cliWhilePaused(const QString &account,const QString &method,const QJsonObject &params) {
    // Only the GUI event loop is paused: the real independent CLI and engine
    // commit B before the native editor authors A against its retained revision.
    const auto path=QDir(m_profile).filePath("composition-interleaved-"+QUuid::createUuid().toString(QUuid::Id128)+".json");
    QSaveFile file(path); const auto bytes=QJsonDocument(params).toJson();
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes)!=bytes.size() || !file.commit()) {
        finish("Cannot prepare the bounded composition conflict request."); return {};
    }
    QProcess process;
    process.start(cliPath(m_engine),{"--profile",m_profile,"--engine",m_engine,"--account",account,"--json","call",method,"--params-file",path});
    if (!process.waitForStarted(5000) || !process.waitForFinished(10000)) {
        process.kill(); process.waitForFinished(1000); finish("Interleaved composition CLI timed out."); return {};
    }
    const auto response=QJsonDocument::fromJson(process.readAllStandardOutput()).object();
    if (process.exitStatus()!=QProcess::NormalExit || process.exitCode()!=0 || !response["ok"].toBool()) {
        finish("Interleaved composition CLI failed: "+QString::fromUtf8(process.readAllStandardError())); return {};
    }
    return response["result"].toObject();
}
bool GuiCompositionSelfTest::retainDraft(const QJsonObject &draft) {
    const auto actual=durableFields(draft); const auto adapter=durableFields(QJsonObject::fromVariantMap(m_backend.draft()));
    if (actual!=adapter || draft["draft_id"].toString()!=m_draftId || draft["mode"].toString()!=modes[m_mode]
        || draft["body"].toString()!=m_body || draft["subject"].toString()!=m_subject
        || draft["from"].toString()!="you@windscribe.example" || draft["discarded"]!=false || draft["revision"].toInt()<2) {
        finish("Native composition fields and independent CLI differ from authored expectations."); return false;
    }
    if (m_mode==0) {
        if (addresses(draft["to_recipients"])!=QStringList{"alice@example.test"}
            || addresses(draft["cc_recipients"])!=QStringList{"copy@example.test"}
            || addresses(draft["bcc_recipients"])!=QStringList{"blind@example.test"}
            || draft["to_recipients"].toArray().first().toObject()["name"]!="Alice Reader"
            || draft["cc_recipients"].toArray().first().toObject()["name"]!="Copy Reader"
            || draft["bcc_recipients"].toArray().first().toObject()["name"]!="Blind Reader"
            || !draft["conversation_id"].isNull() || !draft["source_message_id"].isNull()) {
            finish("Native new-message recipient chips/removal or independent identity differ."); return false;
        }
        QTextDocument rich; rich.setHtml(draft["body_html"].toString()); QTextCursor cursor(&rich); cursor.setPosition(1);
        if (rich.toPlainText()!=m_body || cursor.charFormat().fontWeight()<QFont::Bold) { finish("Native bold formatting did not survive the saved rich document."); return false; }
    }
    for (const auto &value:m_drafts) if (value.toObject()["draft_id"]==draft["draft_id"]) { finish("Different composition modes reused one draft identity."); return false; }
    m_drafts.append(actual); return true;
}
bool GuiCompositionSelfTest::startRestart() {
    m_restartPath=QDir(m_profile).filePath("composition-restart-"+QUuid::createUuid().toString(QUuid::Id128)+".json");
    QSaveFile file(m_restartPath); const auto bytes=QJsonDocument(QJsonObject{{"account","windscribe"},{"restart_draft",m_drafts.first()}}).toJson();
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes)!=bytes.size() || !file.commit()) { finish("Cannot prepare the composition restart expectation."); return false; }
    connect(&m_child,&QProcess::started,this,[this] { m_childPid=QString::number(m_child.processId()); });
    connect(&m_child,&QProcess::finished,this,[this](int code,QProcess::ExitStatus status) {
        m_processTimeout.stop();
        if (code!=0 || status!=QProcess::NormalExit) { finish("Fresh GUI composition restart failed: "+QString::fromUtf8(m_child.readAllStandardError()+m_child.readAllStandardOutput()).trimmed()); return; }
        QFile file(m_restartPath+".observed.json");
        if (!file.open(QIODevice::ReadOnly) || file.size()>65536) { finish("Fresh GUI returned no bounded composition observation."); return; }
        const auto observed=QJsonDocument::fromJson(file.readAll()).object();
        if (observed["gui_process_id"].toString()!=m_childPid || !observed["native_editor_verified"].toBool() || observed["draft"]!=m_drafts.first()) {
            finish("Fresh GUI did not restore the independently authored draft and native editor."); return;
        }
        m_childDone=true;
    });
    connect(&m_child,&QProcess::errorOccurred,this,[this](auto) { finish("Cannot launch fresh composition GUI."); });
    m_processTimeout.start();
    m_child.start(QCoreApplication::applicationFilePath(),{"--smoke","--fixture-clock","--profile",m_profile,"--engine",m_engine,"--self-test-composition-restart",m_restartPath});
    return true;
}
void GuiCompositionSelfTest::restore() {
    m_backend.selectAccount(m_originalAccount); m_backend.setFolder(m_originalFolder); m_backend.setQuery(m_originalQuery);
    m_backend.setCategory(m_originalCategory); m_backend.applyFilters(m_originalFilters);
    if (!m_originalDraftId.isEmpty()) m_backend.openDraft(m_originalDraftId); else m_backend.selectConversation(m_originalSelected);
}
void GuiCompositionSelfTest::step() {
    if (m_lastStage!=m_stage) { m_lastStage=m_stage; m_ticks=0; qInfo()<<"Native composition stage"<<m_stage<<"mode"<<modes.value(m_mode,"source-conflict"); }
    if (++m_ticks>250) { finish(QString("Native composition timed out at stage %1 (mode %2, status %3).").arg(m_stage).arg(modes.value(m_mode,"source-conflict"),m_backend.draftStatus())); return; }
    if (m_waiting || !m_backend.connected()) return;
    switch (m_stage) {
    case 0:
        if (!saved()) return;
        m_originalDraft=m_backend.draft(); m_originalView=m_backend.presentation(); m_originalFilters=m_backend.filters();
        m_originalAccount=m_backend.account(); m_originalSelected=m_backend.selectedId(); m_originalDraftId=m_backend.activeDraftId();
        m_originalFolder=m_backend.folder(); m_originalQuery=m_backend.query(); m_originalCategory=m_backend.category();
        m_originalFocus=m_window->activeFocusItem(); m_originalWorkspace=m_window->property("workspace").toString(); m_originalSettings=m_window->property("settingsOpen").toBool();
        m_window->setProperty("settingsOpen",false); m_window->setProperty("workspace","mail");
        m_backend.selectAccount("windscribe"); m_backend.setFolder("inbox"); m_backend.setQuery(""); m_backend.setCategory(""); m_backend.applyFilters({}); m_stage=m_attachmentsOnly?70:1; break;
    case 1:
        if (!saved() || !click("newMessageButton")) return; m_stage=2; break;
    case 2:
        if (!saved() || m_backend.draft()["mode"].toString()!="new" || !item("replyComposer")) return;
        m_draftId=m_backend.activeDraftId();
        if (m_draftId.isEmpty() || !m_backend.selectedId().isEmpty()) { finish("Native New message did not create an independent draft."); return; }
        m_stage=42; cli({"draft","show","--draft-id",m_draftId},[this](auto draft) { m_invalidSaved=durableFields(draft); }); break;
    case 42:
        m_backend.setProperty("selfTestExpectedError",true);
        if (!recipient("to_recipients","unfinished-recipient")) return;
        m_stage=43; break;
    case 43:
        if (m_backend.error().isEmpty()) return;
        if (m_backend.activeDraftId()!=m_draftId || m_backend.account()!="windscribe"
            || plain(item("draftRecipient_to_recipients"))!="unfinished-recipient"
            || m_backend.recipientInputs()["to_recipients"].toString()!="unfinished-recipient"
            || !visibleCenter(withText(m_window->contentItem(),m_backend.error()),m_window)) {
            finish("Incomplete native recipient did not retain its text, draft identity and visible validation error."); return;
        }
        m_stage=44; cli({"draft","show","--draft-id",m_draftId},[this](auto draft) {
            if (durableFields(draft)!=m_invalidSaved) finish("Incomplete recipient changed the independently saved draft fields or revision.");
        }); break;
    case 44:
        if (!recipient("to_recipients","Alice Reader <alice@example.test>")) return;
        m_stage=45; break;
    case 45:
        if (!saved()) return;
        if (!m_backend.error().isEmpty() || !m_backend.recipientInputs().isEmpty()
            || !withTip(m_window->contentItem(),"Remove alice@example.test")) {
            finish("Correcting the same incomplete input did not create the valid chip and clear validation."); return;
        }
        m_backend.setProperty("selfTestExpectedError",false);
        if (!recipient("to_recipients","Discard Me <discard@example.test>")) return;
        qInfo()<<"Incomplete native recipient retained exact input and saved revision; correcting the same field created its chip.";
        m_stage=3; break;
    case 3:
        if (!clickItem(withTip(m_window->contentItem(),"Remove discard@example.test"))) return;
        m_stage=4; break;
    case 4:
        if (!click("draftShowCc") || !click("draftShowBcc")) return; m_stage=5; break;
    case 5:
        m_subject="Native composition new"; m_body="Native new message — Montréal 日本語.\nOnly the human wrote this.";
        if (!recipient("cc_recipients","Copy Reader <copy@example.test>") || !recipient("bcc_recipients","Blind Reader <blind@example.test>")
            || !type("draftSubject",m_subject) || !type("replyComposer",m_body)) return;
        if (!QMetaObject::invokeMethod(item("replyComposer"),"select",Q_ARG(int,0),Q_ARG(int,6))) { finish("Native composer selection is unavailable."); return; }
        key(item("replyComposer"),Qt::Key_B,Qt::ControlModifier); m_stage=6; break;
    case 6:
        if (!saved() || m_backend.draftStatus()!="Saved on this machine") return;
        m_stage=7; cli({"draft","show","--draft-id",m_draftId},[this](auto draft) { retainDraft(draft); }); break;
    case 7:
        if (!click("captureReply")) return; m_stage=8; break;
    case 8:
        if (!saved() || !m_backend.captureStatus().contains("accepted")) return;
        m_stage=40; cli({"draft","history","--draft-id",m_draftId},[this](auto result) {
            const auto rows=result["captures"].toArray();
            if (rows.size()!=1 || rows.first().toObject()["state"]!="accepted" || rows.first().toObject()["provider_delivered"].toBool()) { finish("Native composition capture is missing, duplicated or claims delivery."); return; }
            m_captureOperation=rows.first().toObject();
        }); break;
    case 40:
        m_stage=41;
        cli({"draft","capture","--draft-id",m_draftId,"--request-id",m_captureOperation["operation_id"].toString(),"--expected-revision",QString::number(m_captureOperation["draft_revision"].toInteger())},[this](auto receipt) {
            QString error; const auto draft=m_drafts.last().toObject();
            if (!verifyMime(m_profile,draft,receipt,error)) { finish(error); return; }
            m_captures.append(QJsonObject{{"draft_id",m_draftId},{"operation",m_captureOperation},{"receipt",receipt}});
        }); break;
    case 41: m_stage=m_afterCapture; break;
    case 9:
        if (!click("draftsScopeButton")) return; m_stage=10; break;
    case 10:
        if (!saved() || !click("conversationRow_"+m_draftId)) return; m_stage=11; break;
    case 11:
        if (!saved() || plain(item("replyComposer"))!=m_body || m_backend.activeDraftId()!=m_draftId) return;
        if (!click("accountTab_personal")) return; m_stage=12; break;
    case 12:
        if (!saved() || m_backend.account()!="personal") return;
        if (m_backend.activeDraftId()==m_draftId) { finish("New composition leaked into another account."); return; }
        if (!click("accountTab_windscribe")) return; m_stage=13; break;
    case 13:
        if (!saved() || m_backend.account()!="windscribe" || m_backend.activeDraftId()!=m_draftId || plain(item("replyComposer"))!=m_body) return;
        m_stage=14; cli({"call","view_get"},[this](auto result) {
            if (result["view"].toObject()["selectedDraft"].toString()!=m_draftId) finish("Account navigation did not persist the selected independent draft.");
        }); break;
    case 14:
        if (m_backend.preferencesPending()) return;
        if (startRestart()) m_stage=15; break;
    case 15:
        if (!m_childDone) return;
        m_backend.setFolder("inbox"); m_backend.setQuery(""); m_backend.selectConversation(sourceConversation); m_stage=16; break;
    case 16:
        if (!saved() || m_backend.selectedId()!=sourceConversation || m_backend.conversation()["messages"].toList().isEmpty()) return;
        m_stage=20; m_mode=1; cli({"mail","read",sourceConversation},[this](auto result) { m_sourceBefore=result; }); break;
    case 20: {
        if (!saved()) return;
        // The reader's actions hide while a reply is open; collapse it first.
        const QString action=m_mode==1 ? "replyAllButton" : m_mode==2 ? "forwardButton" : "replyButton";
        if (!item(action)) { click("collapseReply"); return; }
        if (!click(action)) return;
        m_stage=21; break;
    }
    case 21: {
        if (!saved() || m_backend.draft()["mode"].toString()!=modes[m_mode] || m_backend.activeDraftId()==m_draftId) return;
        m_draftId=m_backend.activeDraftId(); const auto draft=QJsonObject::fromVariantMap(m_backend.draft());
        const auto expectedTo=m_mode==1 ? QStringList{"status@service.example","alex@windscribe.example"} : m_mode==2 ? QStringList{} : QStringList{"status@service.example"};
        if (addresses(draft["to_recipients"])!=expectedTo || !addresses(draft["cc_recipients"]).isEmpty()
            || draft["source_message_id"].toString()!="windscribe-message-005-000" || draft["conversation_id"].toString()!=sourceConversation
            || draft["subject"].toString()!=(m_mode==2 ? "Fwd: " : "Re: ")+QString("Weekly service availability")
            || (m_mode!=2 && draft["reply_to_message_id"].toString()!="<html-only@fixture.example>")) {
            finish("Native reply/forward routing did not derive the exact retained MIME source."); return;
        }
        m_body="Native "+modes[m_mode]+" note — Montréal 日本語.";
        if (m_mode==2) {
            const auto quote=draft["body"].toString();
            // Exact normalized text of fixture html-only.eml, independently
            // confirmed through the real CLI; its heading shares the first line.
            const QString expectedQuote="---------- Forwarded message ----------\nFrom: status@service.example\nDate: \nSubject: Weekly service availability\n\n"
                "> Weekly availability99.97% uptime & one resolved incident.\n> Read the report\n> Review the Frankfurt maintenance window.";
            if (quote!=expectedQuote) { finish("Forward did not preserve the exact normalized quote of the retained source."); return; }
            m_body+="\n\n"+quote;
            if (!recipient("to_recipients","Forward Reader <forward@example.test>")) return;
        }
        m_subject="Native composition "+modes[m_mode];
        if (!type("draftSubject",m_subject) || !type("replyComposer",m_body)) return;
        m_afterCapture=22; m_stage=6; break;
    }
    case 22:
        if (++m_mode<4) { m_stage=20; break; }
        m_stage=50;
        cli({"mail","read",sourceConversation},[this](auto result) { if (result!=m_sourceBefore) finish("Composition or local capture changed received messages or their read/star/sent flags."); }); break;
    case 50:
        if (!saved()) return;
        m_backend.selectAccount("personal"); m_stage=51; break;
    case 51:
        if (!saved() || m_backend.account()!="personal") return;
        m_backend.setFolder("inbox"); m_backend.setQuery(""); m_backend.setCategory(""); m_backend.applyFilters({});
        m_backend.selectConversation(conflictConversation); m_stage=52; break;
    case 52:
        if (!saved() || m_backend.selectedId()!=conflictConversation
            || m_backend.conversation()["messages"].toList().size()!=32) return;
        if (m_backend.draft()["revision"].toInt()!=0 || m_backend.draft()["source_message_id"].toString()!=conflictSourceA) {
            finish("Frozen-source conflict requires the unused virtual reply to the latest Personal member A."); return;
        }
        if (!click("replyButton")) return;
        m_stage=53; break;
    case 53: {
        if (!saved() || !item("replyComposer") || !item("draftSubject") || !item("draftRecipient_to_recipients")) return;
        const auto virtualDraft=durableFields(QJsonObject::fromVariantMap(m_backend.draft()));
        if (virtualDraft["revision"].toInt()!=0 || virtualDraft["source_message_id"].toString()!=conflictSourceA
            || virtualDraft["conversation_id"].toString()!=conflictConversation || virtualDraft["mode"]!="reply") {
            finish("Opening the native virtual reply changed the required source A context."); return;
        }
        m_backend.setProperty("selfTestExpectedError",true);
        const auto other=cliWhilePaused("personal","draft_save",{{"conversation_id",conflictConversation},
            {"source_message_id",conflictSourceB},{"expected_revision",0},{"request_id","native-first-save-source-b"},
            {"subject","Independent CLI source B subject"},{"body","Independent CLI source B draft"},
            {"body_html",QJsonValue::Null},{"signature_text","Source B signature"},{"attachment_ids",QJsonArray{}}});
        if (other.isEmpty()) return;
        m_frozenStored=durableFields(other);
        if (other["source_message_id"].toString()!=conflictSourceB || other["revision"].toInt()!=1
            || other["source_hash"]==virtualDraft["source_hash"] || other["reply_to_message_id"]==virtualDraft["reply_to_message_id"]) {
            finish("Independent first save did not freeze distinct source B and its reply identity."); return;
        }
        if (!type("draftSubject","Local source A subject")
            || !recipient("to_recipients","Context A <local-a@example.test>")
            || !type("replyComposer","Retained source A edits — Montréal 日本語.")) {
            finish("Could not author native fields before processing the independent source B commit."); return;
        }
        m_frozenLocal=durableFields(QJsonObject::fromVariantMap(m_backend.draft()));
        if (m_frozenLocal["revision"].toInt()!=0 || m_frozenLocal["source_message_id"].toString()!=conflictSourceA
            || m_frozenLocal["source_hash"]!=virtualDraft["source_hash"]
            || m_frozenLocal["reply_to_message_id"]!=virtualDraft["reply_to_message_id"]
            || !addresses(m_frozenLocal["to_recipients"]).contains("local-a@example.test")) {
            finish("Authoring native fields changed frozen source A before its stale save."); return;
        }
        m_backend.saveDraft(); m_stage=54; break;
    }
    case 54:
        if (m_backend.draftBusy() || !m_backend.draftConflict()) return;
        if (durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_frozenLocal
            || plain(item("replyComposer"))!=m_frozenLocal["body"].toString()
            || plain(item("draftSubject"))!=m_frozenLocal["subject"].toString()) {
            finish("Stale first save changed the native source A draft or its editor fields."); return;
        }
        if (!click("saveMyDraftVersion")) return;
        m_stage=55; break;
    case 55: {
        auto dialog=m_window->findChild<QObject *>("draftConflictDialog");
        if (!dialog || !dialog->property("opened").toBool() || !click("confirmSaveMyDraft")) return;
        m_stage=56; break;
    }
    case 56:
        if (m_backend.draftBusy() || m_backend.draftStatus()!=frozenContextRefusal) return;
        if (!m_backend.draftConflict() || durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_frozenLocal
            || plain(item("replyComposer"))!=m_frozenLocal["body"].toString()
            || plain(item("draftSubject"))!=m_frozenLocal["subject"].toString()
            || !withText(m_window->contentItem(),frozenContextRefusal)) {
            finish("Confirmed replacement did not refuse a different frozen source while retaining exact local fields and revision."); return;
        }
        m_stage=57;
        cli({"draft","show",conflictConversation},[this](auto result) {
            if (durableFields(result)!=m_frozenStored) finish("Refused source-context rebase modified the independent source B draft or revision.");
        },"personal"); break;
    case 57:
        // Cleanup explicitly chooses B only after proving Save my version kept
        // A and the independently saved B unchanged; no implicit replacement.
        if (!clickItem(buttonWithText(m_window->contentItem(),"Reload saved draft…"))) return;
        m_stage=58; break;
    case 58:
        if (!withText(m_window->contentItem(),"Reload the saved draft?")
            || !clickItem(buttonWithText(m_window->contentItem(),"Reload"))) return;
        m_stage=59; break;
    case 59:
        if (!saved() || m_backend.draftConflict()) return;
        if (durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_frozenStored) {
            finish("Explicit reload did not select the independently saved source B fields unchanged."); return;
        }
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError",false);
        qInfo()<<"Native frozen-source conflict passed: virtual member A/revision 0, independent first save of B/revision 1, stale authoring, actual Save my version confirmation refused, all local fields and both source/revision identities unchanged; explicit Reload cleanup selected B.";
        m_stage=23; break;
    case 23:
        restore(); m_stage=24; break;
    case 24:
        if (!saved() || m_backend.account()!=m_originalAccount || m_backend.selectedId()!=m_originalSelected) return;
        if (durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=durableFields(QJsonObject::fromVariantMap(m_originalDraft))) { finish("Composition walkthrough changed the protected pre-existing draft."); return; }
        for (auto it=m_originalView.cbegin();it!=m_originalView.cend();++it) m_backend.remember(it.key(),it.value());
        m_window->setProperty("workspace",m_originalWorkspace); m_window->setProperty("settingsOpen",m_originalSettings);
        if (m_originalFocus) m_originalFocus->forceActiveFocus();
        if (m_attachmentsOnly) qInfo()<<"Native attachment handlers passed: flush, chips/removal, cached queries during held auxiliary response, provider-pause cancellation isolation, late cached-read/text/account retention, exact retry after source deletion, discarded-reply revival and synthetic capture. Physical OS picker and live provider pause unverified.";
        else qInfo()<<"Native composition passed: new/reply/reply-all/forward, recipient chips/subject/rich text, Drafts/account restoration, fresh GUI auto-restoration, four exact local MIME captures and unchanged received flags.";
        finish(); break;
    case 70:
        if (!saved() || m_backend.account()!="windscribe" || !click("newMessageButton")) return;
        m_stage=71; break;
    case 71:
        if (!saved() || m_backend.draft()["mode"]!="new" || m_backend.activeDraftId().isEmpty()) return;
        if (m_attachmentDraft.isEmpty()) {
            m_attachmentDraft=m_backend.activeDraftId();
            m_attachmentPath=QDir(m_profile).filePath("native-attachment-"+QUuid::createUuid().toString(QUuid::Id128)+".bin");
            m_attachmentRetryPath=m_attachmentPath+".retry";
            m_attachmentBytes=QByteArray::fromHex("4e6174697665206174746163686d656e742000ff0d0a");
            m_attachmentRetryBytes=QByteArray::fromHex("52657472792065786163742062797465732000800a");
            for (const auto &pair: {qMakePair(m_attachmentPath,m_attachmentBytes),qMakePair(m_attachmentRetryPath,m_attachmentRetryBytes)}) {
                QFile file(pair.first);
                if (!file.open(QIODevice::WriteOnly|QIODevice::NewOnly) || file.write(pair.second)!=pair.second.size()) { finish("Cannot create the owned native attachment fixture."); return; }
            }
        }
        if (!type("draftSubject","Native imported attachments") || !type("replyComposer","Before local import")
            || !recipient("to_recipients","attachment@example.test")) return;
        m_stage=72; break;
    case 72:
        if (!saved() || !click("draftAttachButton")) return;
        m_stage=73; break;
    case 73: {
        auto menu=m_window->findChild<QObject *>("draftAttachmentMenu");
        auto choice=m_window->findChild<QObject *>("draftAttachLocalFile");
        if (!menu || !menu->property("opened").toBool() || !choice || !choice->property("enabled").toBool()) return;
        QMetaObject::invokeMethod(menu,"close");
        auto dialog=m_window->findChild<QObject *>("draftAttachmentFileDialog");
        if (!dialog || !QMetaObject::invokeMethod(dialog,"rejected")) { finish("Native attachment picker cancellation handler is unavailable."); return; }
        if (m_backend.attachmentImportBusy() || m_backend.activeDraftId()!=m_attachmentDraft) { finish("Cancelling a picker changed the draft or started import."); return; }
        if (!type("replyComposer","Authored immediately before import")) return;
        if (!attachmentSelection(m_attachmentPath,"windscribe",m_attachmentDraft)) return;
        m_stage=74; break;
    }
    case 74:
        if (!saved() || m_backend.attachmentImportPending() || m_backend.draft()["attachment_ids"].toList().size()!=1) return;
        m_firstAttachment=m_backend.draft()["attachment_ids"].toList().first().toString();
        if (!item("removeDraftAttachment_"+m_firstAttachment)) { finish("Imported file did not become a removable native chip."); return; }
        m_stage=75; cli({"draft","show","--draft-id",m_attachmentDraft},[this](auto draft) {
            if (draft["body"]!="Authored immediately before import" || draft["attachment_ids"].toArray().size()!=1
                || durableFields(draft)!=durableFields(QJsonObject::fromVariantMap(m_backend.draft()))) finish("Import did not flush authored edits before advancing the exact saved draft.");
        }); break;
    case 75: {
        // Capture an actual cached-connection read before the second import.
        // Delay only its production callback, leaving both real IPC owners free.
        m_heldDraftGeneration=m_backend.m_generation;
        m_backend.loadActiveDraft();
        auto pending=m_backend.m_pending.find(QString::number(m_backend.m_sequence));
        if (pending==m_backend.m_pending.end() || pending->method!="draft_get") { finish("Cannot hold the real cached draft read before import."); return; }
        m_heldDraftCallback=std::move(pending->callback);
        pending->callback=[self=QPointer<GuiCompositionSelfTest>(this)](const QJsonObject &result,bool ok) {
            if (!self) return;
            if (!ok) { self->finish("Cached draft read failed before the delayed projection check."); return; }
            self->m_heldDraftResult=result;
        };
        m_stage=93; break;
    }
    case 93:
        if (m_heldDraftResult.isEmpty()) return;
        if (durableFields(m_heldDraftResult)!=durableFields(QJsonObject::fromVariantMap(m_backend.draft()))) { finish("Held cached draft result was not the exact pre-import projection."); return; }
        if (!attachmentSelection(m_attachmentRetryPath,"windscribe",m_attachmentDraft,true)) return;
        m_attachmentIntent=m_backend.m_attachmentImports.value(m_backend.attachmentImportKey());
        m_stage=76; break;
    case 76:
        if (!m_heldAttachment) { finish("Attachment owner disappeared before delayed response check."); return; }
        if (!m_heldAttachment->m_hello) {
            if (m_heldAttachment->m_socket.bytesAvailable()>0) m_heldAttachment->receive();
            return;
        }
        if (m_heldAttachment->m_socket.bytesAvailable()==0) return;
        if (!type("replyComposer","New local text typed while the import receipt is delayed")) return;
        m_backend.setQuery("Weekly service availability"); m_stage=77; break;
    case 77:
        if (m_backend.loading() || m_backend.query()!="Weekly service availability") return;
        if (m_backend.conversations().size()!=1 || !m_backend.attachmentImportBusy()
            || m_backend.draft()["body"]!="New local text typed while the import receipt is delayed") {
            finish("Cached query did not progress independently or delayed import replaced new text."); return;
        }
        if (!item("captureReply") || item("captureReply")->isEnabled() || !item("discardReply") || item("discardReply")->isEnabled()) {
            finish("Native capture/discard stayed enabled before the selected attachment outcome was resolved."); return;
        }
        // Exercise the production cancellation path used by Pause, without
        // fabricating a live account or starting any provider work.
        m_backend.cancelGoogleProviderOperations("windscribe");
        if (!m_heldAttachment || !m_backend.attachmentImportBusy() || m_heldAttachment->m_socket.state()!=QLocalSocket::ConnectedState) {
            finish("Pausing provider operations cancelled an unrelated local attachment import."); return;
        }
        m_stage=78; cli({"draft","show","--draft-id",m_attachmentDraft},[this](auto draft) {
            if (draft["attachment_ids"].toArray().size()!=2 || draft["body"]!="Authored immediately before import") { finish("Held auxiliary receipt did not correspond to a committed import and deferred newer editor text."); return; }
            m_backend.setProperty("selfTestExpectedError",true);
            m_heldAttachment->m_socket.abort(); m_heldAttachment.clear();
            if (!QFile::remove(m_attachmentRetryPath)) finish("Cannot remove only the owned retry source fixture.");
        }); break;
    case 78:
        if (m_backend.attachmentImportBusy() || !m_backend.attachmentImport()["retry"].toBool()) return;
        if (m_backend.m_attachmentImports.value(m_backend.attachmentImportKey())!=m_attachmentIntent
            || m_backend.draft()["body"]!="New local text typed while the import receipt is delayed"
            || !m_backend.attachmentImport()["status"].toString().startsWith("Attachment result not confirmed")) { finish("Uncertain import lost its exact request identity/text or incorrectly claimed the file was not attached."); return; }
        if (!click("retryDraftAttachment")) return;
        m_stage=79; break;
    case 79:
        if (!saved() || m_backend.attachmentImportPending()) return;
        if (m_backend.draft()["attachment_ids"].toList().size()!=2 || m_backend.draft()["body"]!="New local text typed while the import receipt is delayed") { finish("Exact receipt replay duplicated an attachment or replaced newer body text."); return; }
        {
            const auto before=m_backend.draft();
            if (!m_heldDraftCallback || m_backend.m_draftDirty || m_backend.m_generation!=m_heldDraftGeneration
                || m_heldDraftResult["revision"].toInteger()>=before["revision"].toLongLong()) { finish("Delayed cached-read regression did not reach the same clean draft with a newer import revision."); return; }
            auto callback=std::move(m_heldDraftCallback); m_heldDraftCallback={};
            callback(m_heldDraftResult,true);
            if (m_backend.draft()!=before || m_backend.draftConflict()) { finish("Delayed cached draft read rolled back a committed attachment or newer saved text."); return; }
            qInfo()<<"Actual cached draft projection revision"<<m_heldDraftResult["revision"].toInteger()<<"delivered after import/replay/save revision"<<before["revision"].toLongLong()<<"left the clean native draft unchanged.";
        }
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError",false); m_backend.setQuery("");
        m_stage=80; break;
    case 80:
        if (!saved() || !attachmentSelection(m_attachmentPath,"windscribe",m_attachmentDraft,true)) return;
        m_stage=81; break;
    case 81:
        if (!m_heldAttachment) { finish("Late-account import lost its owned connection."); return; }
        if (!m_heldAttachment->m_hello) { if (m_heldAttachment->m_socket.bytesAvailable()>0) m_heldAttachment->receive(); return; }
        if (m_heldAttachment->m_socket.bytesAvailable()==0 || !click("accountTab_personal")) return;
        m_stage=82; break;
    case 82:
        if (!saved() || m_backend.account()!="personal" || !click("newMessageButton")) return;
        m_stage=83; break;
    case 83:
        if (!saved() || m_backend.draft()["mode"]!="new" || m_backend.activeDraftId().isEmpty()) return;
        m_attachmentOtherDraft=m_backend.activeDraftId();
        if (!type("draftSubject","Other account attachment isolation") || !type("replyComposer","This other draft stays unchanged")) return;
        m_stage=84; break;
    case 84:
        if (!saved()) return;
        m_attachmentOther=durableFields(QJsonObject::fromVariantMap(m_backend.draft()));
        releaseAttachment();
        // The original picker accepted after account navigation uses its old
        // target; the production handler must refuse to retarget this draft.
        if (!attachmentSelection(m_attachmentPath,"windscribe",m_attachmentDraft)) return;
        m_stage=85; break;
    case 85:
        if (m_backend.attachmentImportBusy()) return;
        if (m_backend.account()!="personal" || m_backend.activeDraftId()!=m_attachmentOtherDraft
            || durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_attachmentOther
            || plain(item("replyComposer"))!="This other draft stays unchanged") { finish("Late import/picker result retargeted the other account's native draft."); return; }
        m_stage=86; cli({"draft","show","--draft-id",m_attachmentOtherDraft},[this](auto draft) {
            if (durableFields(draft)!=m_attachmentOther) { finish("Late import changed the independent other-account stored draft."); return; }
            m_attachmentDrafts.append(draft);
        },"personal"); break;
    case 86:
        if (!click("accountTab_windscribe")) return;
        m_stage=87; break;
    case 87:
        if (!saved() || m_backend.account()!="windscribe") return;
        if (m_backend.activeDraftId()!=m_attachmentDraft) { m_backend.openDraft(m_attachmentDraft); return; }
        if (m_backend.draft()["attachment_ids"].toList().size()!=3) return;
        m_stage=95; break;
    case 95: {
        if (!visibleCenter(item("replyComposer"),m_window)) { if (auto editor=item("replyComposer")) reveal(editor); return; }
        if (!saved() || !attachmentSelection(m_attachmentPath,"windscribe",m_attachmentDraft,true)) return;
        const auto before=QJsonObject::fromVariantMap(m_backend.draft());
        m_frozenStored=cliWhilePaused("windscribe","draft_save",{{"draft_id",m_attachmentDraft},
            {"expected_revision",before["revision"]},{"request_id","native-attachment-stale-"+QUuid::createUuid().toString(QUuid::Id128)},
            {"body","Independent CLI attachment conflict"},{"body_html",QJsonValue::Null},
            {"signature_text",before["signature_text"]},{"attachment_ids",before["attachment_ids"]}});
        if (m_frozenStored.isEmpty()) return;
        if (!type("replyComposer","Local text retained on stale attachment import")) { finish("Cannot focus the native editor for stale import authoring."); return; }
        m_frozenLocal=durableFields(QJsonObject::fromVariantMap(m_backend.draft()));
        m_backend.setProperty("selfTestExpectedError",true); releaseAttachment(); m_stage=96; break;
    }
    case 96:
        if (m_backend.attachmentImportBusy() || !m_backend.draftConflict()) return;
        if (m_backend.attachmentImportPending() || durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_frozenLocal
            || plain(item("replyComposer"))!="Local text retained on stale attachment import") { finish("Stale import changed native text, revision, attachments or frozen draft identity."); return; }
        m_stage=97; cli({"draft","show","--draft-id",m_attachmentDraft},[this](auto draft) {
            if (durableFields(draft)!=durableFields(m_frozenStored)) finish("Stale import changed the independently committed CLI version.");
        }); break;
    case 97:
        if (!clickItem(buttonWithText(m_window->contentItem(),"Reload saved draft…"))) return;
        m_stage=98; break;
    case 98:
        if (!withText(m_window->contentItem(),"Reload the saved draft?") || !clickItem(buttonWithText(m_window->contentItem(),"Reload"))) return;
        m_stage=99; break;
    case 99:
        if (!saved() || m_backend.draftConflict()) return;
        if (durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=durableFields(m_frozenStored)) { finish("Explicit stale-import cleanup did not preserve the saved CLI version."); return; }
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError",false);
        if (!click("removeDraftAttachment_"+m_firstAttachment)) return;
        m_stage=88; break;
    case 88:
        if (!saved() || m_backend.draft()["attachment_ids"].toList().size()!=2 || !click("captureReply")) return;
        m_stage=89; break;
    case 89:
        if (!saved() || !m_backend.captureStatus().contains("accepted")) return;
        m_stage=90; cli({"draft","history","--draft-id",m_attachmentDraft},[this](auto result) {
            const auto rows=result["captures"].toArray();
            if (rows.size()!=1 || rows.first().toObject()["state"]!="accepted" || rows.first().toObject()["provider_delivered"].toBool()) { finish("Native attachment capture was duplicated or claimed provider delivery."); return; }
            m_captureOperation=rows.first().toObject();
        }); break;
    case 90:
        m_stage=91; cli({"draft","show","--draft-id",m_attachmentDraft},[this](auto draft) {
            if (durableFields(draft)!=durableFields(QJsonObject::fromVariantMap(m_backend.draft()))) { finish("Native attachment removal or later text differs from saved draft."); return; }
            m_attachmentDrafts.append(draft);
            const auto ids=draft["attachment_ids"].toArray();
            m_attachmentFiles={QJsonObject{{"account","windscribe"},{"id",ids[0]},{"base64",QString::fromLatin1(m_attachmentRetryBytes.toBase64())}},
                QJsonObject{{"account","windscribe"},{"id",ids[1]},{"base64",QString::fromLatin1(m_attachmentBytes.toBase64())}}};
        }); break;
    case 91:
        m_stage=92; cli({"draft","capture","--draft-id",m_attachmentDraft,"--request-id",m_captureOperation["operation_id"].toString(),"--expected-revision",QString::number(m_captureOperation["draft_revision"].toInteger())},[this](auto receipt) {
            QString error;
            if (!verifyMime(m_profile,m_attachmentDrafts.last().toObject(),receipt,error)) { finish(error); return; }
            QFile file(receipt["path"].toString());
            if (!file.open(QIODevice::ReadOnly)) { finish("Cannot inspect the exact native attachment MIME export."); return; }
            auto bytes=file.readAll(); bytes.replace("\r\n","");
            if (!bytes.contains(m_attachmentBytes.toBase64()) || !bytes.contains(m_attachmentRetryBytes.toBase64())) { finish("Capture did not retain the exact imported binary bytes after source deletion."); return; }
            m_attachmentCapture={{"draft_id",m_attachmentDraft},{"operation",m_captureOperation},{"receipt",receipt}};
        }); break;
    case 92: {
        // Prepare a separate real discarded legacy reply, then exercise Attach
        // directly after reopening it, without a typed edit to revive it first.
        auto draft=cliWhilePaused("sideproject","draft_get",{{"conversation_id","sideproject-thread-003"}});
        if (draft.isEmpty()) return;
        draft=cliWhilePaused("sideproject","draft_save",{{"conversation_id","sideproject-thread-003"},{"expected_revision",draft["revision"]},
            {"source_message_id",draft["source_message_id"]},{"request_id","native-attachment-reopen-save"},{"body","Discarded attachment test reply"}});
        if (draft.isEmpty()) return;
        m_frozenStored=cliWhilePaused("sideproject","draft_discard",{{"conversation_id","sideproject-thread-003"},
            {"expected_revision",draft["revision"]},{"request_id","native-attachment-reopen-discard"}});
        if (m_frozenStored.isEmpty()) return;
        m_backend.selectAccount("sideproject"); m_stage=100; break;
    }
    case 100:
        if (!saved() || m_backend.account()!="sideproject") return;
        m_backend.selectConversationInternal("sideproject-thread-003",false); m_stage=101; break;
    case 101:
        if (m_backend.selectedId()!="sideproject-thread-003" || m_backend.draftBusy() || m_backend.loading()
            || !m_backend.draft()["discarded"].toBool()) return;
        if (durableFields(QJsonObject::fromVariantMap(m_backend.draft()))!=durableFields(m_frozenStored)) { finish("Reopened legacy reply did not load the exact discarded projection."); return; }
        if (!click("replyButton")) return;
        m_stage=102; break;
    case 102:
        if (!item("replyComposer") || !attachmentSelection(m_attachmentPath,"sideproject",m_frozenStored["draft_id"].toString())) return;
        m_stage=103; break;
    case 103:
        if (!saved() || m_backend.attachmentImportPending()) return;
        if (m_backend.draft()["discarded"].toBool() || m_backend.draft()["attachment_ids"].toList().size()!=1
            || m_backend.draft()["revision"].toLongLong()!=m_frozenStored["revision"].toInteger()+2
            || m_backend.draft()["source_message_id"].toString()!=m_frozenStored["source_message_id"].toString()
            || !m_backend.draft()["body"].toString().isEmpty()) { finish("Attach did not save/revive the discarded legacy reply once before importing without changing its source/body."); return; }
        m_stage=104; cli({"draft","show","--draft-id",m_frozenStored["draft_id"].toString()},[this](auto draft) {
            if (durableFields(draft)!=durableFields(QJsonObject::fromVariantMap(m_backend.draft()))) { finish("Reopened reply attachment differs from the independently saved draft."); return; }
            m_attachmentDrafts.append(draft);
            m_attachmentFiles.append(QJsonObject{{"account","sideproject"},{"id",draft["attachment_ids"].toArray().first()},
                {"base64",QString::fromLatin1(m_attachmentBytes.toBase64())}});
        },"sideproject"); break;
    case 104: m_stage=23; break;
    }
}

bool GuiCompositionSelfTest::attachmentSelection(const QString &path,const QString &account,const QString &draft,bool hold) {
    auto dialog=m_window->findChild<QObject *>("draftAttachmentFileDialog");
    if (!dialog) { finish("The native composer has no local file-selection handler."); return false; }
    dialog->setProperty("targetAccount",account); dialog->setProperty("targetDraft",draft);
    dialog->setProperty("selectedFile",QUrl::fromLocalFile(path));
    if (!QMetaObject::invokeMethod(dialog,"accepted")) { finish("The native local-file accepted handler was not callable."); return false; }
    if (hold) {
        for (auto operation:m_backend.m_googleOperations) {
            if (operation->m_method!="draft_attachment_import") continue;
            m_heldAttachment=operation;
            // Hold real IPC delivery, not a fake Core result. The authenticated
            // hello is released separately and CLI reads prove the commit.
            QObject::disconnect(&operation->m_socket,&QLocalSocket::readyRead,operation,&GoogleOperation::receive);
            break;
        }
        if (!m_heldAttachment) { finish("Could not hold the dedicated attachment response after a clean save."); return false; }
    }
    return true;
}
void GuiCompositionSelfTest::releaseAttachment() {
    auto operation=m_heldAttachment; m_heldAttachment.clear();
    if (!operation) return;
    QObject::connect(&operation->m_socket,&QLocalSocket::readyRead,operation,&GoogleOperation::receive);
    operation->receive();
}

bool GuiCompositionSelfTest::verifyRestartProjection(MailBackend &backend,QQuickWindow *window,const QString &expectedPath,QString &error) {
    QFile file(expectedPath);
    if (!QDir::isAbsolutePath(expectedPath) || !file.open(QIODevice::ReadOnly) || file.size()>65536) { error="Cannot read bounded composition restart expectation."; return false; }
    const auto expected=QJsonDocument::fromJson(file.readAll()).object(); const auto draft=expected["restart_draft"].toObject();
    const auto actual=durableFields(QJsonObject::fromVariantMap(backend.draft()));
    if (!window || draft["draft_id"].toString().isEmpty() || expected["account"]!="windscribe" || backend.account()!="windscribe"
        || backend.activeDraftId()!=draft["draft_id"].toString() || actual!=draft || backend.presentation()["selectedDraft"]!=draft["draft_id"].toString()
        || window->property("workspace").toString()!="mail" || window->property("settingsOpen").toBool()) {
        error="Fresh GUI did not naturally restore the saved account/draft identity and exact fields."; return false;
    }
    auto editor=find(window->contentItem(),"replyComposer"); auto subject=find(window->contentItem(),"draftSubject");
    if (!editor || !subject || plain(editor)!=draft["body"].toString() || subject->property("text").toString()!=draft["subject"].toString()
        || !find(window->contentItem(),"draftRecipient_to_recipients")) { error="Fresh native composition controls did not hydrate the saved draft."; return false; }
    reveal(editor);
    // A restored editor can be taller than its scroll area, so require a real
    // visible part of it rather than its centre.
    auto shown=editor->mapRectToScene(QRectF(0,0,editor->width(),editor->height())).intersected(QRectF(0,0,window->width(),window->height()));
    for (auto ancestor=editor->parentItem(); ancestor; ancestor=ancestor->parentItem())
        if (ancestor->clip()) shown=shown.intersected(ancestor->mapRectToScene(QRectF(0,0,ancestor->width(),ancestor->height())));
    if (!editor->isVisible() || shown.width()<20 || shown.height()<20) { error="Restored native editor has no visible viewport."; return false; }
    QSaveFile output(expectedPath+".observed.json");
    const auto bytes=QJsonDocument(QJsonObject{{"draft",actual},{"gui_process_id",QString::number(QCoreApplication::applicationPid())},{"native_editor_verified",true}}).toJson();
    if (!output.open(QIODevice::WriteOnly) || output.write(bytes)!=bytes.size() || !output.commit()) { error="Cannot retain the fresh GUI composition observation."; return false; }
    qInfo()<<"Fresh GUI naturally restored the account's selected independent draft and exact native authored fields."; return true;
}
bool GuiCompositionSelfTest::verifyRetained(const QString &profile,const QString &engine,const QVariantMap &expected,QString &error) {
    const auto data=QJsonObject::fromVariantMap(expected); const auto drafts=data["drafts"].toArray();
    if (drafts.size()!=4 || data["captures"].toArray().size()!=4) { error="Composition retention expects all four native draft modes and captures."; return false; }
    const auto read=[&](const QStringList &arguments) {
        QProcess process; process.start(cliPath(engine),QStringList{"--profile",profile,"--engine",engine,"--account","windscribe","--json"}+arguments);
        if (!process.waitForStarted(5000) || !process.waitForFinished(10000)) { process.kill(); process.waitForFinished(1000); error="Composition restart CLI timed out."; return QJsonObject{}; }
        const auto reply=QJsonDocument::fromJson(process.readAllStandardOutput()).object();
        if (process.exitStatus()!=QProcess::NormalExit || process.exitCode()!=0 || !reply["ok"].toBool()) { error="Composition restart CLI failed."; return QJsonObject{}; }
        return reply["result"].toObject();
    };
    for (const auto &value:drafts) {
        const auto draft=value.toObject(); const auto actual=read({"draft","show","--draft-id",draft["draft_id"].toString()});
        if (!error.isEmpty() || durableFields(actual)!=draft) { if (error.isEmpty()) error="A native draft changed after final process restart."; return false; }
    }
    for (const auto &value:data["captures"].toArray()) {
        const auto record=value.toObject(); const auto history=read({"draft","history","--draft-id",record["draft_id"].toString()});
        if (!error.isEmpty() || history["captures"].toArray()!=QJsonArray{record["operation"]}) { if (error.isEmpty()) error="A native capture identity or attempt count changed after restart."; return false; }
        QJsonObject draft; for (const auto &row:drafts) if (row.toObject()["draft_id"]==record["draft_id"]) draft=row.toObject();
        if (!verifyMime(profile,draft,record["receipt"].toObject(),error)) return false;
    }
    if (read({"mail","read",sourceConversation})!=data["source_before"].toObject()) { if (error.isEmpty()) error="Composition restart changed received source or flags."; return false; }
    qInfo()<<"Independent final CLI verified all four native composition drafts, exact retained MIME hashes/content, one-attempt capture identities and unchanged source flags after process restart.";
    return true;
}

bool GuiCompositionSelfTest::verifyAttachments(const QString &profile,const QString &engine,const QVariantMap &expected,QString &error) {
    const auto data=QJsonObject::fromVariantMap(expected);
    const auto drafts=data["attachment_drafts"].toArray(), files=data["attachment_files"].toArray();
    if (drafts.size()!=3 || files.size()!=3) { error="Attachment verification requires three exact drafts and three retained binary selections."; return false; }
    const auto read=[&](const QString &account,const QStringList &arguments) {
        QProcess process; process.start(cliPath(engine),QStringList{"--profile",profile,"--engine",engine,"--account",account,"--json"}+arguments);
        if (!process.waitForStarted(5000) || !process.waitForFinished(10000)) { process.kill(); process.waitForFinished(1000); error="Attachment restart CLI timed out."; return QJsonObject{}; }
        const auto response=QJsonDocument::fromJson(process.readAllStandardOutput()).object();
        if (process.exitStatus()!=QProcess::NormalExit || process.exitCode()!=0 || !response["ok"].toBool()) { error="Attachment restart CLI failed."; return QJsonObject{}; }
        return response["result"].toObject();
    };
    for (const auto &value:drafts) {
        const auto draft=value.toObject();
        const auto actual=read(draft["account"].toString(),{"draft","show","--draft-id",draft["draft_id"].toString()});
        if (!error.isEmpty() || durableFields(actual)!=durableFields(draft)) { error="Imported attachment draft identity or authored fields changed after engine restart."; return false; }
    }
    for (const auto &value:files) {
        const auto file=value.toObject(); const auto attachment=read(file["account"].toString(),{"attachment",file["id"].toString()});
        QFile blob(attachment["path"].toString());
        if (!error.isEmpty() || attachment["state"]!="available" || !blob.open(QIODevice::ReadOnly) || blob.size()>1024
            || blob.readAll()!=QByteArray::fromBase64(file["base64"].toString().toLatin1())) { error="Restart lost or changed retained imported file bytes."; return false; }
    }
    const auto capture=data["attachment_capture"].toObject();
    const auto history=read("windscribe",{"draft","history","--draft-id",capture["draft_id"].toString()});
    if (!error.isEmpty() || history["captures"].toArray()!=QJsonArray{capture["operation"]}) { error="Imported-file capture identity or attempt count changed after restart."; return false; }
    QJsonObject authored; for (const auto &value:drafts) if (value.toObject()["draft_id"]==capture["draft_id"]) authored=value.toObject();
    if (!verifyMime(profile,authored,capture["receipt"].toObject(),error)) return false;
    qInfo()<<"Independent CLI after actual GUI/engine close verified three draft identities including the reopened legacy reply, exact binary bytes, one synthetic capture receipt and authored text. No real send/upload or physical picker claim.";
    return true;
}
