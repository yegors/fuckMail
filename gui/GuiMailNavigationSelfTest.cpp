// Developer harness: the mail search and navigation stage of `fuckmail-gui
// --self-test`. Types searches and filters into the real UI, opens results from
// known demo messages, and checks read-on-open, request ordering and scrolling.
#include "GuiMailNavigationSelfTest.h"
#include <QGuiApplication>
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QImage>
#include <QFile>
#include <QFileInfo>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJSValue>
#include <QQuickTextDocument>
#include <QTextDocument>
#include <QScopedValueRollback>
#include <QUuid>
#include <array>

namespace {
struct SearchCase { const char *query; const char *thread; const char *message; const char *body; int members; };
// Independent expected source identities from fixture 2, not a projection copied
// from the query under test. The final body is the eighteenth of 32 messages.
constexpr std::array<SearchCase, 4> searches{{
    {"tomasz@windscribe.example", "windscribe-thread-001", "windscribe-message-001-000", "Revised paywall copy is ready", 1},
    {"from:Aisha Rahman", "windscribe-thread-002", "windscribe-message-002-000", "3 of 4 recommend hire", 1},
    {"WireGuard relay rollout", "windscribe-thread-003", "windscribe-message-003-000", "Toronto is clean since Thursday", 1},
    {"longthread17", "windscribe-thread-004", "windscribe-message-004-017", "Member 18 of 32.", 32}
}};
QQuickItem *find(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name && parent->isVisible()) return parent;
    for (auto child : parent->childItems()) if (auto result = find(child, name)) return result;
    return nullptr;
}
QString renderedText(QQuickItem *item) {
    const auto document=qvariant_cast<QQuickTextDocument *>(item->property("textDocument"));
    return document ? document->textDocument()->toPlainText() : item->property("text").toString();
}
QQuickItem *bodyItem(QQuickItem *parent, const QString &expected) {
    if (parent->isVisible() && parent->property("readOnly").toBool()
        && renderedText(parent).simplified().contains(expected.simplified())) return parent;
    for (auto child : parent->childItems()) if (auto result = bodyItem(child, expected)) return result;
    return nullptr;
}
QQuickItem *textItem(QQuickItem *parent, const QString &text) {
    if (parent->isVisible() && parent->property("text").toString()==text) return parent;
    for (auto child : parent->childItems()) if (auto found=textItem(child,text)) return found;
    return nullptr;
}
void key(QObject *target, int value) {
    QKeyEvent press(QEvent::KeyPress,value,Qt::NoModifier), release(QEvent::KeyRelease,value,Qt::NoModifier);
    QCoreApplication::sendEvent(target,&press); QCoreApplication::sendEvent(target,&release);
}
bool fullyVisible(QQuickItem *item, QQuickWindow *window) {
    if (!item || !item->isVisible() || item->width() <= 0 || item->height() <= 0) return false;
    const auto rect = item->mapRectToScene(item->boundingRect()).adjusted(1, 1, -1, -1);
    if (!QRectF(0, 0, window->width(), window->height()).contains(rect)) return false;
    for (auto ancestor = item->parentItem(); ancestor; ancestor = ancestor->parentItem())
        if (ancestor->clip() && !ancestor->mapRectToScene(ancestor->boundingRect()).contains(rect)) return false;
    return true;
}
bool inputViewportVisible(QQuickItem *item,QQuickWindow *window) {
    if (!item || !item->isVisible() || item->width()<=0 || item->height()<=0) return false;
    // TextInput's boundingRect follows its horizontally scrolled glyphs. The
    // editable control keeps its own viewport when retained input is longer.
    const auto rect=item->mapRectToScene(QRectF(0,0,item->width(),item->height())).adjusted(1,1,-1,-1);
    if (!QRectF(0,0,window->width(),window->height()).contains(rect)) return false;
    for (auto ancestor=item->parentItem();ancestor;ancestor=ancestor->parentItem())
        if (ancestor->clip() && !ancestor->mapRectToScene(QRectF(0,0,ancestor->width(),ancestor->height())).contains(rect)) return false;
    return true;
}
QString geometryDiagnostic(QQuickItem *item,QQuickWindow *window) {
    QJsonArray ancestors;
    for (auto current=item; current && ancestors.size()<8; current=current->parentItem()) {
        const auto rect=current->mapRectToScene(current->boundingRect());
        QJsonObject geometry{{"name",current->objectName()},{"type",current->metaObject()->className()},
            {"x",rect.x()},{"y",rect.y()},{"width",rect.width()},{"height",rect.height()},
            {"control_width",current->width()},{"control_height",current->height()},
            {"implicit_width",current->implicitWidth()},{"implicit_height",current->implicitHeight()},
            {"visible",current->isVisible()},{"enabled",current->isEnabled()},{"clip",current->clip()}};
        for (const auto key:{"contentX","contentWidth","originX","contentY","contentHeight","originY"}) {
            const auto value=current->property(key);
            if (value.isValid()) geometry[key]=QJsonValue::fromVariant(value);
        }
        ancestors.append(geometry);
    }
    return QString::fromUtf8(QJsonDocument(QJsonObject{{"found",item!=nullptr},
        {"fully_visible",fullyVisible(item,window)},{"input_viewport_visible",inputViewportVisible(item,window)},
        {"window_width",window->width()},{"window_height",window->height()},
        {"text_length",item ? item->property("text").toString().size() : -1},{"ancestors",ancestors}})
        .toJson(QJsonDocument::Compact));
}
QJsonObject draftFields(const QJsonObject &draft) {
    QJsonObject fields;
    for (const auto name:{"body","body_html","signature_text","attachment_ids","from_name","from","to","subject","identity_revision","revision"}) fields[name]=draft[name];
    return fields;
}
QSet<QString> messageIds(const QJsonArray &values) {
    QSet<QString> ids;
    for (const auto &value:values) ids.insert(value.toString());
    return ids;
}
const QString arrivalBody=QString::fromUtf8("Retain this reply during arrival — Montréal, 日本語.\nHuman draft; no submission.");
}

GuiMailNavigationSelfTest::GuiMailNavigationSelfTest(MailBackend &backend, QQuickWindow *window,
    std::function<void(const QString &)> finished, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_finished(std::move(finished)) {
    m_timer.setInterval(80);
    connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    connect(m_window, &QQuickWindow::frameSwapped, this, [this] { ++m_renderedFrames; });
    connect(&m_backend, &MailBackend::loadingChanged, this, [this] {
        if (m_searchPending && m_backend.query() == m_expectedQuery && m_backend.loading()) m_loadingSeen = true;
    });
    // A real socket write precedes its response. Inspect only the actual active
    // query, not loadingChanged (which fires before request construction).
    connect(&m_backend.m_socket, &QLocalSocket::bytesWritten, this, [this] { holdInFlightResponse(); observeArrivalReceipt(); holdDeferredFlagReceipt(); });
    m_arrivalCliTimeout.setSingleShot(true); m_arrivalCliTimeout.setInterval(10000);
    connect(&m_arrivalCliTimeout,&QTimer::timeout,this,[this] { finish("Independent snapshot-arrival CLI readback timed out."); });
    m_timer.start();
}
void GuiMailNavigationSelfTest::finish(const QString &error) {
    if (!m_finished) return;
    m_timer.stop();
    if (m_accountPressHeld) {
        m_accountPressHeld=false;
        const QPointF outside(-10,-10), global=m_window->mapToGlobal(outside.toPoint());
        QMouseEvent move(QEvent::MouseMove,outside,global,Qt::NoButton,Qt::LeftButton,Qt::NoModifier);
        QMouseEvent release(QEvent::MouseButtonRelease,outside,global,Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
        QCoreApplication::sendEvent(m_window,&move); QCoreApplication::sendEvent(m_window,&release);
    }
    if (m_queueCheckActive) { m_queueCheckActive = false; m_backend.m_pollTimer.start(); }
    m_arrivalCliTimeout.stop(); m_arrivalCli.disconnect(this);
    if (m_arrivalCli.state()!=QProcess::NotRunning) m_arrivalCli.kill();
    m_holdArmed = false;
    if (m_responseHeld && m_heldDelegate) { auto delegate=std::move(m_heldDelegate); m_heldDelegate={}; delegate(m_heldResult,m_heldOk); }
    m_deferredHoldArmed = false;
    if (m_deferredHeld && m_deferredDelegate) {
        auto delegate=std::move(m_deferredDelegate); m_deferredDelegate={}; delegate(m_deferredReceipt,m_deferredOk);
    }
    if (m_compactOriginalSize.isValid()) { m_window->resize(m_compactOriginalSize); m_compactOriginalSize={}; }
    auto callback = std::move(m_finished); m_finished = {};
    deleteLater(); callback(error);
}
bool GuiMailNavigationSelfTest::retainNativeRow(const QString &id, bool requireScrolled) {
    const auto list=find(m_window->contentItem(),"conversationList");
    const auto row=find(m_window->contentItem(),"conversationRow_"+id);
    if (!list || !fullyVisible(row,m_window)) return false;
    m_retainedRow=row;
    m_retainedListY=list->property("contentY").toDouble();
    m_retainedRowY=row->mapToScene(QPointF()).y();
    return !requireScrolled || m_retainedListY>list->property("originY").toDouble()-list->property("topMargin").toDouble()+1;
}
QString GuiMailNavigationSelfTest::retainedNativeRowError(const QString &id) const {
    const auto list=find(m_window->contentItem(),"conversationList");
    const auto row=find(m_window->contentItem(),"conversationRow_"+id);
    if (!list || !m_retainedRow || row!=m_retainedRow || !fullyVisible(row,m_window))
        return "The native mail delegate was recreated or left the viewport during a same-view update.";
    const auto tolerance=1.0/m_window->devicePixelRatio()+0.01;
    if (qAbs(list->property("contentY").toDouble()-m_retainedListY)>tolerance
        || qAbs(row->mapToScene(QPointF()).y()-m_retainedRowY)>tolerance)
        return "The native mail viewport moved during a same-position row update: "+geometryDiagnostic(row,m_window);
    QVariant projection=row->parentItem()->property("rowData");
    if (projection.metaType()==QMetaType::fromType<QJSValue>()) projection=projection.value<QJSValue>().toVariant();
    QVariantMap expected;
    for (const auto &value:m_backend.conversations()) if (value.toMap()["id"].toString()==id) expected=value.toMap();
    if (expected.isEmpty() || QJsonObject::fromVariantMap(projection.toMap())!=QJsonObject::fromVariantMap(expected))
        return "The retained native mail delegate did not receive the current canonical row fields.";
    return {};
}
QString GuiMailNavigationSelfTest::retainedAccountTabError(bool pressed) const {
    const auto tab=find(m_window->contentItem(),"accountTab_"+m_retainedAccount);
    if (!m_retainedAccountTab || tab!=m_retainedAccountTab || !fullyVisible(tab,m_window))
        return "The account tab was recreated or left the viewport during the real count refresh.";
    const auto rect=tab->mapRectToScene(QRectF(0,0,tab->width(),tab->height()));
    const auto tolerance=1.0/m_window->devicePixelRatio()+0.01;
    if (qAbs(rect.x()-m_retainedAccountRect.x())>tolerance || qAbs(rect.y()-m_retainedAccountRect.y())>tolerance
        || qAbs(rect.width()-m_retainedAccountRect.width())>tolerance || qAbs(rect.height()-m_retainedAccountRect.height())>tolerance)
        return "The unchanged target account tab moved during the origin account's count refresh: "+geometryDiagnostic(tab,m_window);
    if (tab->property("pressed").toBool()!=pressed)
        return "The account tab lost its native press across the real count refresh.";
    QVariant projection=tab->property("rowData");
    if (projection.metaType()==QMetaType::fromType<QJSValue>()) projection=projection.value<QJSValue>().toVariant();
    QVariantMap expected;
    for (const auto &value:m_backend.accounts()) if (value.toMap()["id"].toString()==m_retainedAccount) expected=value.toMap();
    if (expected.isEmpty() || QJsonObject::fromVariantMap(projection.toMap())!=QJsonObject::fromVariantMap(expected))
        return "The retained account tab does not carry its current canonical account identity.";
    return {};
}
bool GuiMailNavigationSelfTest::typeQuery(const QString &query) {
    auto field = find(m_window->contentItem(), "searchField");
    if (!field || !field->isEnabled() || !fullyVisible(field, m_window)) return false;
    m_expectedQuery = query; m_loadingSeen = false; m_searchPending = true;
    field->forceActiveFocus(); QMetaObject::invokeMethod(field, "selectAll");
    if (query.isEmpty()) {
        QKeyEvent press(QEvent::KeyPress, Qt::Key_Backspace, Qt::NoModifier), release(QEvent::KeyRelease, Qt::Key_Backspace, Qt::NoModifier);
        QCoreApplication::sendEvent(field, &press); QCoreApplication::sendEvent(field, &release);
    } else {
        QInputMethodEvent input; input.setCommitString(query); QCoreApplication::sendEvent(field, &input);
    }
    return field->property("text").toString() == query && m_backend.query() == query;
}
bool GuiMailNavigationSelfTest::click(const QString &name) {
    return clickItem(find(m_window->contentItem(), name));
}
bool GuiMailNavigationSelfTest::clickItem(QQuickItem *control,Qt::MouseButton button) {
    // Do not scroll from the test to conceal a missing native reveal path.
    if (!control || !control->isEnabled()) return false;
    const bool editable=control->property("cursorPosition").isValid() && !control->property("readOnly").toBool();
    if (!(editable ? inputViewportVisible(control,m_window) : fullyVisible(control,m_window))) return false;
    const auto position = control->mapToScene({control->width()/2, control->height()/2});
    const QPointF global = m_window->mapToGlobal(position.toPoint());
    QMouseEvent movement(QEvent::MouseMove, position, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    QMouseEvent press(QEvent::MouseButtonPress, position, global, button, button, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, button, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &movement);
    QCoreApplication::sendEvent(m_window, &press); QCoreApplication::sendEvent(m_window, &release);
    return true;
}
QObject *GuiMailNavigationSelfTest::senderPopup() const {
    for (auto object:m_window->findChildren<QObject *>("senderCard")) if (object->property("opened").toBool()) return object;
    return nullptr;
}
bool GuiMailNavigationSelfTest::hoverSender() {
    auto sender=find(m_window->contentItem(),"senderFilter_windscribe-message-005-000");
    if (!fullyVisible(sender,m_window)) return false;
    const auto position=sender->mapToScene({sender->width()/2,sender->height()/2});
    QMouseEvent movement(QEvent::MouseMove,position,QPointF(m_window->mapToGlobal(position.toPoint())),
        Qt::NoButton,Qt::NoButton,Qt::NoModifier);
    QCoreApplication::sendEvent(m_window,&movement);
    return true;
}
bool GuiMailNavigationSelfTest::filterResultMatches() const {
    const auto rows=m_backend.conversations();
    if (rows.size()!=1) return false;
    const auto row=rows.first().toMap();
    return row["id"].toString()==searches[0].thread && row["group"].toString()=="done"
        && row["matching_message_ids"].toList()==QVariantList{QString(searches[0].message)};
}
void GuiMailNavigationSelfTest::holdInFlightResponse() {
    if (!m_obsoleteRequestId.isEmpty() && m_backend.m_inFlight==m_obsoleteRequestId) m_obsoleteSent=true;
    if (!m_holdArmed || m_backend.account()!=m_originalAccount || m_backend.query()!=searches[1].query) return;
    auto it=m_backend.m_pending.find(m_backend.m_inFlight);
    if (it==m_backend.m_pending.end() || it->method!="conversations" || it->account!=m_originalAccount
        || it->started<=0 || it->listGeneration!=m_backend.m_listGeneration) return;
    m_heldRequestId=m_backend.m_inFlight; m_heldGeneration=it->listGeneration;
    m_heldDelegate=it->callback;
    const auto original=m_heldDelegate;
    QPointer<GuiMailNavigationSelfTest> guard(this);
    it->callback=[guard,original](const QJsonObject &result,bool ok) {
        if (!guard) { original(result,ok); return; }
        guard->m_heldResult=result; guard->m_heldOk=ok; guard->m_responseHeld=true;
    };
    m_holdArmed=false; m_responseWrapped=true;
    // Forty actual inputs/forced query loads exceed the transport's 32-slot cap
    // while this genuine request remains active. Only the last unsent generation
    // may survive; no fake busy marker or fabricated response participates.
    for (int index=0;index<40;++index) {
        if (!typeQuery(index%2 ? searches[1].query : searches[0].query)) { finish("Queue-pressure input did not reach the real search field."); return; }
        m_backend.m_searchTimer.stop(); // This test explicitly dispatches each debounce result.
        const auto previousSequence=m_backend.m_sequence;
        m_backend.loadConversations();
        const auto id=QString::number(m_backend.m_sequence);
        const auto queued=m_backend.m_pending.find(id);
        if (m_backend.m_sequence<=previousSequence || queued==m_backend.m_pending.end() || queued->method!="conversations" || queued->started!=0) {
            finish("Superseding query was not admitted under real in-flight queue pressure."); return;
        }
        if (index==0) {
            m_obsoleteRequestId=id; const auto obsoleteDelegate=queued->callback;
            queued->callback=[guard,obsoleteDelegate](const QJsonObject &result,bool ok) {
                if (guard) guard->m_obsoleteCanceled=!ok && result.isEmpty() && !guard->m_obsoleteSent;
                obsoleteDelegate(result,ok);
            };
        }
        if (index==39) m_finalRequestId=id;
    }
    auto final=m_backend.m_pending.find(m_finalRequestId);
    const auto finalDelegate=final->callback;
    final->callback=[guard,finalDelegate](const QJsonObject &result,bool ok) {
        if (guard) { guard->m_finalResult=result; guard->m_finalOk=ok; guard->m_finalReceived=true; }
        finalDelegate(result,ok);
    };
}
void GuiMailNavigationSelfTest::observeArrivalReceipt() {
    if (!m_arrivalDoneArmed) return;
    auto pending=m_backend.m_pending.find(m_backend.m_inFlight);
    if (pending==m_backend.m_pending.end() || pending->method!="flag" || pending->account!=m_arrivalAccount || pending->started<=0) return;
    m_arrivalDoneArmed=false;
    const auto original=pending->callback;
    QPointer<GuiMailNavigationSelfTest> guard(this);
    pending->callback=[guard,original](const QJsonObject &result,bool ok) {
        if (guard) { guard->m_arrivalReceipt=result; guard->m_arrivalReceiptOk=ok; guard->m_arrivalReceiptSeen=true; }
        original(result,ok);
    };
}
void GuiMailNavigationSelfTest::holdDeferredFlagReceipt() {
    if (!m_deferredHoldArmed || m_backend.selectedId()!=m_deferredSecond) return;
    auto pending=m_backend.m_pending.find(m_backend.m_inFlight);
    if (pending==m_backend.m_pending.end() || pending->method!="flag" || pending->account!=m_originalAccount || pending->started<=0) return;
    m_deferredHoldArmed=false; m_deferredDelegate=pending->callback;
    const auto original=m_deferredDelegate;
    QPointer<GuiMailNavigationSelfTest> guard(this);
    pending->callback=[guard,original](const QJsonObject &result,bool ok) {
        if (!guard) { original(result,ok); return; }
        guard->m_deferredReceipt=result; guard->m_deferredOk=ok; guard->m_deferredHeld=true;
    };
}
void GuiMailNavigationSelfTest::arrivalCli(const QStringList &arguments,std::function<void(const QJsonObject &)> callback) {
    m_timer.stop(); m_arrivalCli.disconnect(this);
    connect(&m_arrivalCli,&QProcess::finished,this,[this,callback](int code,QProcess::ExitStatus status) {
        m_arrivalCliTimeout.stop();
        const auto reply=QJsonDocument::fromJson(m_arrivalCli.readAllStandardOutput()).object();
        if (code!=0 || status!=QProcess::NormalExit || !reply["ok"].toBool()) {
            finish("Independent snapshot-arrival CLI readback failed: "+QString::fromUtf8(m_arrivalCli.readAllStandardError())); return;
        }
        QPointer<GuiMailNavigationSelfTest> guard(this);
        callback(reply["result"].toObject());
        if (guard && m_finished) m_timer.start();
    });
    connect(&m_arrivalCli,&QProcess::errorOccurred,this,[this](auto) { finish("Cannot run the independent snapshot-arrival CLI."); });
    const auto program=QDir(QFileInfo(m_backend.m_engine).absolutePath()).filePath("fm"
#ifdef Q_OS_WIN
        ".exe"
#endif
    );
    m_arrivalCli.start(program,QStringList{"--profile",m_backend.m_profile,"--engine",m_backend.m_engine,"--account",m_arrivalAccount,"--json"}+arguments);
    m_arrivalCliTimeout.start();
}
QJsonObject GuiMailNavigationSelfTest::arrivalReport() const {
    QFile report(m_arrivalReportPath);
    if (!report.open(QIODevice::ReadOnly) || report.size()>1024*1024) return {};
    return QJsonDocument::fromJson(report.readAll()).object();
}
bool GuiMailNavigationSelfTest::arrivalMembersMatch(const QJsonObject &conversation,bool starred,bool includeArrival) const {
    auto expected=messageIds(m_arrivalOriginalIds);
    if (includeArrival) {
        if (m_arrivalId.isEmpty() || expected.contains(m_arrivalId)) return false;
        expected.insert(m_arrivalId);
    }
    const auto members=conversation["messages"].toArray();
    if (conversation["id"].toString()!=m_arrivalThread || members.size()!=expected.size()
        || messageIds(conversation["message_ids"].toArray())!=expected) return false;
    QSet<QString> seen;
    for (const auto &value:members) {
        const auto member=value.toObject(); const auto id=member["id"].toString();
        if (!expected.contains(id) || seen.contains(id)) return false;
        seen.insert(id);
        const bool arrived=includeArrival && id==m_arrivalId;
        if (member["unread"].toBool()!=(arrived && !m_arrivalRead) || member["starred"].toBool()!=(arrived?false:starred)) return false;
    }
    return seen==expected;
}
bool GuiMailNavigationSelfTest::arrivalCountsMatch() const {
    const auto account=QJsonObject::fromVariantMap(m_backend.currentAccount());
    const auto query=QJsonObject::fromVariantMap(m_backend.querySummary().value("account_counts").toMap());
    for (auto it=m_arrivalExpectedCounts.begin();it!=m_arrivalExpectedCounts.end();++it) if (account[it.key()]!=it.value()) return false;
    for (const auto key:{"conversation_count","unread_conversation_count","held_conversation_count","done_conversation_count"})
        if (query[key]!=m_arrivalExpectedCounts[key]) return false;
    return true;
}
bool GuiMailNavigationSelfTest::resultMatches() const {
    const auto &expected = searches[m_case];
    const auto rows = m_backend.conversations();
    if (rows.size() != 1) return false;
    const auto row = rows.first().toMap();
    const auto matches = row["matching_message_ids"].toList();
    return row["id"].toString() == expected.thread && matches.size() == 1 && matches.first().toString() == expected.message;
}
bool GuiMailNavigationSelfTest::memberIsReadable() const {
    const auto &expected = searches[m_case];
    if (m_backend.selectedId() != expected.thread) return false;
    const auto messages = m_backend.conversation()["messages"].toList();
    if (messages.size() != expected.members) return false;
    bool sourceFound = false;
    for (const auto &value : messages) {
        const auto source = value.toMap();
        if (source["id"].toString() == expected.message && source["text"].toString().contains(expected.body)) sourceFound = true;
    }
    const auto member = find(m_window->contentItem(), "messageMember_" + QString(expected.message));
    const auto reader = find(m_window->contentItem(), "reader");
    if (!sourceFound || !member || !reader || !member->property("isExpanded").toBool()) return false;
    const auto body = bodyItem(member, expected.body);
    if (!fullyVisible(body, m_window)) return false;
    const auto rect = body->mapRectToItem(reader, body->boundingRect());
    // Checking the full, short fixture body proves the actual match can be read,
    // rather than merely finding a hidden delegate or an onscreen subject row.
    return rect.top() >= -1 && rect.bottom() <= reader->height()+1
        && (m_case != 3 || reader->property("contentY").toDouble() > 100);
}
void GuiMailNavigationSelfTest::restore() {
    m_searchPending = false;
    m_backend.setFolder(m_originalFolder); m_backend.setCategory(m_originalCategory);
    m_backend.applyFilters(m_originalFilters); m_backend.setQuery(m_originalQuery);
    // Passive restoration must not review the original conversation again.
    m_backend.selectConversation(m_originalSelected);
}
void GuiMailNavigationSelfTest::beginQueueCheck(bool fairness) {
    // Queue real requests before yielding the event loop. The first status is
    // already on the actual socket; subsequent reads remain unsent. Only this
    // short ordering check pauses the recurring timer, not the idle/painters.
    m_queueCheckActive = true; m_backend.m_pollTimer.stop();
    m_queueObserved.clear();
    m_queueExpected = fairness
        ? QStringList{"active","q1","q2","q3","q4","poll","q5","q6"}
        : QStringList{"active","q1","poll1","poll2","manual","q2","poll3","write","q3","poll4"};
    QPointer<GuiMailNavigationSelfTest> guard(this);
    const auto queue = [this, guard](const QString &label, const QString &method, bool automatic = false) {
        QJsonObject params;
        QString account;
        if (method == "conversations") { account = m_backend.account(); params = {{"limit",1}}; }
        if (method == "preferences_save") params = {{"preferences",QJsonObject{}},
            {"expected_revision",m_backend.m_preferencesRevision},{"request_id","queue-check-"+QUuid::createUuid().toString(QUuid::Id128)}};
        QScopedValueRollback<bool> origin(m_backend.m_automaticReadScope, automatic);
        m_backend.request(method, account, params, [guard, label, method](const auto &result, bool ok) {
            if (!guard || !guard->m_finished) return;
            if (!ok || result.isEmpty() || (method == "conversations" && result["conversations"].toArray().size()!=1)) {
                guard->finish("Real queued request failed during priority verification: "+label); return;
            }
            if (method == "preferences_save") {
                if (result["preferences"].toObject()!=QJsonObject::fromVariantMap(guard->m_backend.preferences())) {
                    guard->finish("The empty preference-write barrier changed stored preferences."); return;
                }
                guard->m_backend.m_preferencesRevision=result["revision"].toInteger();
            }
            guard->m_queueObserved.append(label);
            if (guard->m_queueObserved!=guard->m_queueExpected.mid(0,guard->m_queueObserved.size()))
                guard->finish("Actual engine response order violated query priority, a read/write barrier or bounded polling progress: "+guard->m_queueObserved.join(","));
        });
    };
    queue("active","status");
    if (fairness) {
        queue("poll","accounts",true);
        for (int index=1;index<=6;++index) queue("q"+QString::number(index),"conversations");
    } else {
        queue("poll1","accounts",true); queue("poll2","preferences_get",true); queue("q1","conversations");
        // Status is always classified as background polling; use an explicit
        // accounts read to exercise the manual-read ordering barrier.
        queue("manual","accounts"); queue("poll3","accounts",true); queue("q2","conversations");
        // Even an accidental automatic scope must never classify a mutation
        // as a bypassable read. The empty patch preserves all user values.
        queue("write","preferences_save",true); queue("poll4","preferences_get",true); queue("q3","conversations");
    }
}
void GuiMailNavigationSelfTest::step() {
    if (++m_ticks > 1500) {
        const auto reader = find(m_window->contentItem(), "reader");
        const auto list = find(m_window->contentItem(), "conversationList");
        finish(QString("Native search timed out at stage %1, case %2 (query=%3, selected=%4, readerY=%5, listY=%6, listContentHeight=%7, listHeight=%8, savedListY=%9).")
            .arg(m_stage).arg(m_case).arg(m_backend.query(), m_backend.selectedId())
            .arg(reader ? reader->property("contentY").toDouble() : -1)
            .arg(list ? list->property("contentY").toDouble() : -1)
            .arg(list ? list->property("contentHeight").toDouble() : -1)
            .arg(list ? list->height() : -1).arg(m_backend.presentation()["listY"].toDouble())); return;
    }
    if (!m_backend.error().isEmpty()) { finish(m_backend.error()); return; }
    if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE") && m_lastStage != m_stage*10+m_case) {
        qInfo() << "Native mail search stage" << m_stage << "case" << m_case; m_lastStage = m_stage*10+m_case;
    }
    switch (m_stage) {
    case 0:
        if (!m_backend.connected() || m_backend.loading() || m_backend.draftBusy() || m_backend.draft().isEmpty()) return;
        if (m_backend.account() != "windscribe") { finish("Native search needs the Windscribe fixture account restored by the preceding helper."); return; }
        m_originalAccount = m_backend.account(); m_originalSelected = m_backend.selectedId();
        m_originalQuery = m_backend.query(); m_originalFolder = m_backend.folder(); m_originalCategory = m_backend.category();
        m_originalFilters = m_backend.filters(); m_originalDraft = m_backend.draft(); m_originalView = m_backend.presentation();
        m_originalFocus = m_window->activeFocusItem();
        m_originalWorkspace = m_window->property("workspace").toString(); m_originalSettings = m_window->property("settingsOpen").toBool();
        m_window->setProperty("settingsOpen", false); m_window->setProperty("workspace", "mail");
        m_backend.setFolder("inbox"); m_backend.setCategory({}); m_backend.applyFilters({});
        m_stage = 80; break;
    case 80:
        if (m_backend.loading() || m_backend.draftBusy() || !m_backend.m_pending.isEmpty()
            || m_backend.m_preferencesSaving || !m_backend.m_preferencesPending.isEmpty()) return;
        beginQueueCheck(false); m_stage=81; break;
    case 81:
        if (m_queueObserved!=m_queueExpected) return;
        if (!m_backend.m_pending.isEmpty()) return;
        beginQueueCheck(true); m_stage=82; break;
    case 82:
        if (m_queueObserved!=m_queueExpected) return;
        m_queueCheckActive=false; m_backend.m_pollTimer.start();
        qInfo() << "Native request ordering passed: real explicit queries preceded unsent automatic reads, retained manual-read/write barriers and delivered polling after four bypasses; stored preferences unchanged.";
        m_stage=90; break;
    case 90:
        if (!m_backend.m_pending.isEmpty() || m_backend.loading() || m_backend.draftBusy() || m_backend.mutationInFlight()) return;
        // Control callback order, not source data: a real reviewed row will
        // leave this real Review-filtered query while the next flag receipt is
        // held. This exercises the same deferred-click race without a 100k seed.
        m_queueCheckActive=true; m_backend.m_pollTimer.stop();
        m_backend.applyFilters({{"status","review"}});
        if (!typeQuery("")) { finish("Cannot prepare the native deferred-click review view."); return; }
        m_stage=91; break;
    case 91: {
        if (m_backend.loading() || m_backend.m_searchTimer.isActive()) return;
        const auto list=find(m_window->contentItem(),"conversationList");
        if (!list) { finish("Deferred-click setup lost the conversation list."); return; }
        // A loaded IPC result precedes QML's model/viewport restoration. Choose
        // source-check candidates from the restored view, not its reset origin.
        // The list's range starts at its top margin, which hides the room
        // above the Inbox's first header.
        const auto origin=list->property("originY").toDouble(), minimum=origin-list->property("topMargin").toDouble();
        const auto maximum=qMax(minimum,origin+list->property("contentHeight").toDouble()+list->property("bottomMargin").toDouble()-list->height());
        const auto restored=qBound(minimum,origin+m_backend.presentation()["listY"].toDouble(),maximum);
        if (qAbs(list->property("contentY").toDouble()-restored)>1) return;
        // Changing the filter can intentionally reset the saved view to its
        // origin. Establish the scrolled precondition this check requires.
        if (maximum <= minimum + 1) { finish("Deferred-click fixture needs a scrollable mail list."); return; }
        if (list->property("contentY").toDouble() <= minimum + 1) {
            list->setProperty("contentY", qMin(maximum, minimum + 120));
            QMetaObject::invokeMethod(list, "movementEnded");
            return;
        }
        QStringList candidates;
        for (const auto &value:m_backend.conversations()) {
            const auto row=value.toMap();
            // Leave the later fixed source/read-on-open fixtures untouched.
            const auto id=row["id"].toString();
            if (id.startsWith(m_originalAccount+"-thread-") && id.right(3).toInt()<6) continue;
            if (row["unread"].toBool() && !row["starred"].toBool() && row["message_count"].toInt()==1
                && fullyVisible(find(m_window->contentItem(),"conversationRow_"+id),m_window)) candidates.append(id);
        }
        if (candidates.size()<2) return;
        m_deferredFirst=candidates[0]; m_deferredSecond=candidates[1]; m_arrivalAccount=m_originalAccount;
        m_stage=92;
        arrivalCli({"mail","read",m_deferredFirst},[this](const auto &result) {
            m_deferredSource=result;
            if (result["id"].toString()!=m_deferredFirst || result["messages"].toArray().size()!=1
                || !result["messages"].toArray().first().toObject()["unread"].toBool())
                finish("Independent deferred-click source was not one unread exact-account member.");
        });
        break;
    }
    case 92:
        if (!retainNativeRow(m_deferredFirst,true)) {
            finish("The native delegate-retention check requires a fully visible row in the restored nonzero mail viewport."); return;
        }
        if (!click("conversationRow_"+m_deferredFirst)) {
            finish("The source-verified deferred-click candidate changed geometry before input: " + geometryDiagnostic(find(m_window->contentItem(),"conversationRow_"+m_deferredFirst),m_window)); return;
        }
        m_stage=93; break;
    case 93:
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.mutationInFlight()
            || m_backend.conversation()["id"].toString()!=m_deferredFirst || !m_backend.m_pending.isEmpty()) return;
        if (m_backend.conversation()["messages"].toList().first().toMap()["unread"].toBool()) return;
        if (const auto error=retainedNativeRowError(m_deferredFirst); !error.isEmpty()) { finish(error); return; }
        qInfo() << "Native mail delegate retained through actual read-on-open: identical item, current row fields and nonzero viewport" << m_retainedListY;
        m_deferredHoldArmed=true;
        if (!click("conversationRow_"+m_deferredSecond)) { m_deferredHoldArmed=false; return; }
        m_stage=94; break;
    case 94:
        if (!m_deferredHeld || m_backend.draftBusy() || !m_backend.m_pending.isEmpty()) return;
        if (!m_deferredOk || !m_backend.mutationInFlight() || m_backend.selectedId()!=m_deferredSecond) {
            finish("The deferred-click test did not hold the genuine second reader flag receipt."); return;
        }
        for (qsizetype index=0; index<m_backend.conversations().size(); ++index) {
            const auto row=m_backend.conversations()[index].toMap();
            if (row["id"].toString()==m_deferredFirst) { m_deferredClickedRow=row; m_deferredClickedIndex=int(index); }
        }
        if (!click("conversationRow_"+m_deferredFirst)) { finish("The previously reviewed row disappeared before the controlled deferred click."); return; }
        if (!m_backend.mutationInFlight() || !m_deferredDelegate || m_backend.selectedId()!=m_deferredFirst) {
            finish("A clicked reader did not open immediately while retaining the pending durable mutation receipt."); return;
        }
        m_backend.loadConversations(false,true); m_stage=95; break;
    case 95: {
        if (m_backend.loading() || !m_backend.m_pending.isEmpty()) return;
        bool pinned=false;
        for (const auto &value:m_backend.conversations()) if (value.toMap()["id"].toString()==m_deferredFirst)
            pinned=value.toMap()["pinned"].toBool();
        if (!pinned) { finish("The reviewed reader target was not retained as a pinned row outside the Review query."); return; }
        if (!m_backend.mutationInFlight() || !m_deferredDelegate) { finish("The real flag receipt was released before the exclusion check."); return; }
        auto delegate=std::move(m_deferredDelegate); m_deferredDelegate={}; m_deferredHeld=false;
        delegate(m_deferredReceipt,m_deferredOk); m_stage=96; break;
    }
    case 96: {
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.mutationInFlight()
            || !m_backend.m_pending.isEmpty() || m_backend.conversation().isEmpty()) return;
        const auto expected=m_deferredSource["messages"].toArray().first().toObject();
        const auto actual=m_backend.conversation(); const auto members=actual["messages"].toList();
        QVariantMap found;
        for (const auto &value:m_backend.conversations()) if (value.toMap()["id"].toString()==m_deferredFirst) found=value.toMap();
        if (m_backend.account()!=m_originalAccount || m_backend.selectedId()!=m_deferredFirst || found.isEmpty()
            || m_backend.m_pinnedIndex!=m_deferredClickedIndex
            || found.value("display_group",found["group"])!=m_deferredClickedRow.value("display_group",m_deferredClickedRow["group"])
            || actual["id"].toString()!=m_deferredFirst || members.size()!=1
            || members.first().toMap()["id"].toString()!=expected["id"].toString()
            || members.first().toMap()["text"].toString()!=expected["text"].toString()) {
            finish("A click deferred behind a real flag receipt lost its original visible row/pin or exact reader source after the intervening page refresh."); return;
        }
        auto member=find(m_window->contentItem(),"messageMember_"+expected["id"].toString());
        auto body=member ? bodyItem(member,expected["text"].toString()) : nullptr;
        if (!fullyVisible(body,m_window) || renderedText(body).simplified()!=expected["text"].toString().simplified()) return;
        m_stage=97;
        arrivalCli({"mail","read",m_deferredFirst},[this,expected](const auto &result) {
            const auto members=result["messages"].toArray();
            if (result["id"].toString()!=m_deferredFirst || members.size()!=1
                || members.first().toObject()["id"]!=expected["id"] || members.first().toObject()["text"]!=expected["text"]
                || members.first().toObject()["unread"].toBool() || members.first().toObject()["starred"]!=expected["starred"])
                finish("Independent deferred-click readback changed source, hold flags or deliberate review state.");
        });
        break;
    }
    case 97:
        qInfo() << "Native pending-receipt reader click passed: the reader opens immediately and stays pinned outside the Review query; exact reader body, account, review state and serial write receipt retained.";
        m_backend.applyFilters({}); m_searchPending=false;
        m_queueCheckActive=false; m_backend.m_pollTimer.start(); m_stage=98; break;
    case 98:
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.mutationInFlight() || !m_backend.m_pending.isEmpty()) return;
        m_compactOriginalSize=m_window->size(); m_compactOriginalAccount=m_backend.account();
        for (const auto &row:m_backend.accounts()) m_compactAccounts.append(row.toMap()["id"].toString());
        if (m_compactAccounts.size()<2) { finish("Compact keyboard navigation needs separate fixture accounts."); return; }
        m_window->resize(QSize(1000,650)); m_compactKey=0; m_compactWait=0; m_stage=99; break;
    case 99: {
        if (m_window->size()!=QSize(1000,650) || m_backend.loading() || m_backend.draftBusy() || m_backend.mutationInFlight()) return;
        auto mail=find(m_window->contentItem(),"mailWorkspace");
        if (!mail) { finish("Compact account navigation lost the Mail workspace."); return; }
        // Use the normal shortcut route from neutral workspace focus. No
        // direct cycleAccount call, tab scrolling, or focus in an editor.
        if (m_window->property("searchPopupOpen").toBool()) key(m_window,Qt::Key_Escape);
        mail->forceActiveFocus(Qt::OtherFocusReason);
        if (!m_window->property("accountKeysAllowed").toBool()) { finish("Compact account shortcuts are unavailable from neutral Mail focus."); return; }
        const auto index=m_compactAccounts.indexOf(m_backend.account());
        if (index<0) { finish("Compact account navigation left the fixture inventory."); return; }
        const int direction=m_compactKey<3 ? 1 : -1;
        m_compactExpectedAccount=m_compactAccounts[(index+direction+m_compactAccounts.size())%m_compactAccounts.size()];
        const auto modifiers=direction>0 ? Qt::NoModifier : Qt::ShiftModifier;
        QKeyEvent overrideEvent(QEvent::ShortcutOverride,Qt::Key_Tab,modifiers);
        QKeyEvent press(QEvent::KeyPress,Qt::Key_Tab,modifiers), release(QEvent::KeyRelease,Qt::Key_Tab,modifiers);
        m_compactWait=0; m_stage=100;
        QCoreApplication::sendEvent(m_window,&overrideEvent); QCoreApplication::sendEvent(m_window,&press); QCoreApplication::sendEvent(m_window,&release);
        break;
    }
    case 100: {
        auto tab=find(m_window->contentItem(),"accountTab_"+m_compactExpectedAccount);
        const bool ready=m_backend.account()==m_compactExpectedAccount && !m_backend.loading() && !m_backend.draftBusy()
            && m_backend.currentAccount()["id"].toString()==m_compactExpectedAccount && !m_backend.selectedId().isEmpty()
            && m_backend.conversation()["id"].toString()==m_backend.selectedId();
        if (!ready || !tab || !tab->property("selected").toBool() || !fullyVisible(tab,m_window)) {
            if (++m_compactWait>30) finish("Compact Tab/Shift+Tab did not reveal the complete selected mailbox tab and matching reader: "+geometryDiagnostic(tab,m_window));
            return;
        }
        if (++m_compactKey<6) { m_stage=99; break; }
        if (m_backend.account()!=m_compactOriginalAccount) { finish("Compact forward/backward key cycles did not restore the original account."); return; }
        m_window->resize(m_compactOriginalSize); m_stage=101; break;
    }
    case 101:
        if (m_window->size()!=m_compactOriginalSize || m_backend.loading() || m_backend.draftBusy()) return;
        if (!fullyVisible(find(m_window->contentItem(),"accountTab_"+m_compactOriginalAccount),m_window)) return;
        m_compactOriginalSize={};
        qInfo() << "Native compact account keys passed: three Tab and three Shift+Tab events at 1000x650 retained the complete selected tab and matching account reader; original window size and account restored.";
        m_stage=1; break;
    case 1:
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.mutationInFlight()) return;
        if (!typeQuery(searches[m_case].query)) { finish("Text input did not reach the visible native search field."); return; }
        m_stage = 2; break;
    case 2:
        if (!m_loadingSeen || m_backend.loading() || m_backend.query() != m_expectedQuery) return;
        if (!resultMatches()) { finish(QString("Native query '%1' did not return the independently expected source member.").arg(m_expectedQuery)); return; }
        if (!click("conversationRow_" + QString(searches[m_case].thread))) return;
        m_searchPending = false; m_stable = 0; m_stage = 3; break;
    case 3:
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.mutationInFlight() || !memberIsReadable()) { m_stable = 0; return; }
        if (++m_stable < 2) return;
        if (++m_case < int(searches.size())) { m_stage = 1; break; }
        m_stage = 4; break;
    case 4: {
        const auto directory = qEnvironmentVariable("FM_GUI_TEST_SCREENSHOTS");
        if (!directory.isEmpty() && (!QDir::isAbsolutePath(directory) || !QDir().mkpath(directory)
            || !m_window->grabWindow().save(QDir(directory).filePath("mail-earlier-match.png")))) {
            finish("Cannot save the explicitly requested native earlier-match screenshot."); return;
        }
        if (!typeQuery("")) { finish("Cannot clear native search before filter acceptance."); return; }
        m_stage = 10; break;
    }
    case 10:
        if (!m_loadingSeen || m_backend.loading() || !m_backend.query().isEmpty()) return;
        m_searchPending=false; m_unfilteredRows=m_backend.conversations();
        if (m_unfilteredRows.size()<2 || !m_backend.filters().isEmpty()) { finish("Filter acceptance needs the unfiltered mailbox page."); return; }
        // Typed filters (from:, is:, has:) are part of the query Core reads.
        if (!typeQuery(QString("from:") + searches[0].query + " is:done has:attachment")) { finish("Cannot type the combined search grammar."); return; }
        m_stage=11; break;
    case 11: {
        if (!m_loadingSeen || m_backend.loading() || m_backend.m_searchTimer.isActive()) return;
        const auto terms=m_backend.searchTerms();
        if (!filterResultMatches() || terms.value("from").toStringList()!=QStringList{QString(searches[0].query)}
            || terms.value("is").toString()!="done" || !terms.value("has_attachment").toBool() || !m_backend.filters().isEmpty()) {
            finish("The combined search grammar did not return the exact source member with the requested state."); return;
        }
        if (!typeQuery(QString("from:") + searches[0].query + " is:snoozed")) return;
        m_stage=12; break;
    }
    case 12: {
        if (!m_loadingSeen || m_backend.loading() || m_backend.m_searchTimer.isActive()) return;
        const QString message("is: accepts review, delayed or done");
        if (m_backend.searchError()!=message || !filterResultMatches() || !fullyVisible(textItem(m_window->contentItem(),message),m_window)) {
            finish("An unreadable status did not explain itself in the dropdown while keeping the last results."); return;
        }
        QStringList senders;
        for (int index=0;index<5;++index) senders << QString("from:person%1").arg(index);
        if (!typeQuery(senders.join(' '))) return;
        m_stage=13; break;
    }
    case 13:
        if (!m_loadingSeen || m_backend.loading() || m_backend.m_searchTimer.isActive()) return;
        if (m_backend.searchError()!="Search accepts up to four names or addresses across from:, to: and bare addresses" || !filterResultMatches()) {
            finish("Five from: parts were not refused with the last results kept."); return;
        }
        if (!typeQuery("")) return;
        m_stage=14; break;
    case 14:
        if (!m_loadingSeen || m_backend.loading() || !m_backend.query().isEmpty()) return;
        if (!m_backend.filters().isEmpty() || m_backend.conversations()!=m_unfilteredRows || !m_backend.searchError().isEmpty()
            || m_backend.account()!=m_originalAccount || m_backend.selectedId()!=searches[3].thread) { finish("Clearing the search did not restore the unfiltered mailbox context and retained selection."); return; }
        m_holdArmed=true;
        if (!typeQuery(searches[1].query)) { finish("Cannot initiate the actual query for delayed-response acceptance."); return; }
        m_stage=20; break;
    case 20: {
        if (!m_responseHeld || !m_finalReceived || m_backend.loading()) return;
        const auto heldRows=m_heldResult["conversations"].toArray(), finalRows=m_finalResult["conversations"].toArray();
        if (!m_responseWrapped || !m_heldOk || !m_finalOk || heldRows.size()!=1 || finalRows.size()!=1
            || heldRows[0].toObject()["id"].toString()!=searches[1].thread || finalRows[0].toObject()["id"].toString()!=searches[1].thread
            || !m_obsoleteCanceled || m_obsoleteSent || m_backend.m_pending.contains(m_obsoleteRequestId)
            || m_finalRequestId==m_heldRequestId || m_backend.conversations().size()!=1 || m_backend.conversations().first().toMap()["id"].toString()!=searches[1].thread) {
            finish("Real queued queries were not coalesced to the final expected response before the account switch."); return;
        }
        m_searchPending=false;
        if (!click("accountTab_personal")) return;
        m_stage=21; break;
    }
    case 21: {
        if (m_backend.account()!="personal" || m_backend.loading() || m_backend.draft().isEmpty() || m_backend.conversations().isEmpty()) return;
        for (const auto &row:m_backend.conversations()) if (!row.toMap()["id"].toString().startsWith("personal-")) { finish("The new mailbox initially contains foreign rows."); return; }
        const auto rows=m_backend.conversations(); const auto summary=m_backend.querySummary(), draft=m_backend.draft();
        const auto selected=m_backend.selectedId(), query=m_backend.query(), error=m_backend.error();
        auto delegate=std::move(m_heldDelegate); m_heldDelegate={};
        if (!delegate) { finish("The delayed real query callback was lost."); return; }
        delegate(m_heldResult,m_heldOk);
        if (m_backend.account()!="personal" || m_backend.conversations()!=rows || m_backend.querySummary()!=summary || m_backend.draft()!=draft
            || m_backend.selectedId()!=selected || m_backend.query()!=query || m_backend.error()!=error) { finish("Releasing an actual old-account response changed the current mailbox rows, selection or error state."); return; }
        qInfo() << "Native mail search grammar passed combined from:/is:/has: terms, kept rows with Core's message for refused queries and restored the mailbox on clear. Real response" << m_heldRequestId
            << "from generation" << m_heldGeneration << "was held across a native account switch; forty superseding real queries were admitted; obsolete queued query"
            << m_obsoleteRequestId << "never reached transport and final query" << m_finalRequestId << "returned the expected member.";
        m_stage=22; break;
    }
    case 22:
        for (const auto &row:m_backend.conversations()) if (!row.toMap()["id"].toString().startsWith("personal-")) { finish("A released stale response later replaced personal mailbox rows."); return; }
        if (!click("accountTab_"+m_originalAccount)) return;
        m_stage=23; break;
    case 23:
        if (m_backend.account()!=m_originalAccount || m_backend.loading() || m_backend.draftBusy() || m_backend.draft().isEmpty()) return;
        m_arrivalReportPath=m_backend.property("selfTestArrivalReport").toString();
        if (m_arrivalReportPath.isEmpty()) {
            qInfo() << "Native snapshot arrival skipped: no explicit engine verification scenario was configured.";
            restore(); m_stage=5;
        } else m_stage=24;
        break;
    case 24: {
        const auto report=arrivalReport();
        m_arrivalAccount=report["account"].toString(); m_arrivalThread=report["conversation_id"].toString();
        m_arrivalOriginalIds=report["original_message_ids"].toArray();
        if (report["version"].toInt()!=1 || report["mode"].toString()!="snapshot" || report["status"].toString()!="ready"
            || m_arrivalAccount!=m_originalAccount || m_arrivalThread.isEmpty() || m_arrivalOriginalIds.size()!=2
            || messageIds(m_arrivalOriginalIds).size()!=2 || report["seeded_messages"].toInt()!=2
            || report["trigger_count"].toInt()!=0 || report["committed_messages"].toInt()!=0 || !report["errors"].toArray().isEmpty()) {
            finish("The explicit snapshot-arrival engine report is not a fresh two-member provider scenario."); return;
        }
        m_backend.setFolder("all"); m_backend.setCategory({}); m_backend.applyFilters({});
        if (!typeQuery("Snapshot arrival verification")) { finish("Cannot search for the provider-seeded snapshot thread through the native field."); return; }
        m_stage=25; break;
    }
    case 25:
        if (!m_loadingSeen || m_backend.loading()) return;
        if (m_backend.conversations().size()!=1 || m_backend.conversations().first().toMap()["id"].toString()!=m_arrivalThread) {
            finish("The native search did not find the exact provider-seeded snapshot conversation."); return;
        }
        if (!click("conversationRow_"+m_arrivalThread)) return;
        m_searchPending=false; m_stage=26; break;
    case 26:
        if (m_backend.loading() || m_backend.mutationInFlight() || m_backend.draftBusy() || m_backend.draft().isEmpty()
            || m_backend.selectedId()!=m_arrivalThread) return;
        if (!arrivalMembersMatch(QJsonObject::fromVariantMap(m_backend.conversation()),true,false)) return;
        m_stage=27;
        arrivalCli({"mail","read",m_arrivalThread},[this](const auto &source) {
            if (!arrivalMembersMatch(source,true,false)) finish("Independent CLI did not observe native read-on-open preserving both original stars.");
        });
        break;
    case 27:
        if (!click("replyButton")) return;
        m_stage=28; break;
    case 28: {
        const auto editor=find(m_window->contentItem(),"replyComposer");
        if (!editor || !editor->isEnabled() || !inputViewportVisible(editor,m_window)) return;
        // A fresh reply is unsaved and starts with only the account signature.
        if (m_backend.draft()["revision"].toLongLong()!=0) { finish("The fresh provider conversation unexpectedly already has an authored draft."); return; }
        editor->forceActiveFocus(); QMetaObject::invokeMethod(editor,"selectAll"); QInputMethodEvent input; input.setCommitString(arrivalBody); QCoreApplication::sendEvent(editor,&input);
        if (m_backend.draft()["body"].toString()!=arrivalBody) { finish("Unicode native input did not reach the snapshot-arrival draft."); return; }
        m_backend.saveDraft(); m_stage=29; break;
    }
    case 29:
        if (m_backend.draftBusy() || m_backend.draftStatus()!="Saved on this machine") return;
        m_arrivalDraft=draftFields(QJsonObject::fromVariantMap(m_backend.draft()));
        if (m_arrivalDraft["body"].toString()!=arrivalBody) { finish("Saving changed the native snapshot-arrival draft."); return; }
        m_stage=30;
        arrivalCli({"draft","show",m_arrivalThread},[this](const auto &draft) {
            if (draftFields(draft)!=m_arrivalDraft) finish("Independent CLI did not recover the exact new Unicode draft and identity snapshot.");
        });
        break;
    case 30:
        if (auto mail=find(m_window->contentItem(),"mailWorkspace"); mail && mail->property("replyOpen").toBool()) {
            QMetaObject::invokeMethod(mail, "minimizeComposer"); return;
        }
        if (!find(m_window->contentItem(),"unholdConversationButton")) {
            clickItem(find(m_window->contentItem(),"conversationActionsRow"),Qt::RightButton); return;
        }
        if (!click("unholdConversationButton")) return;
        m_stage=31; break;
    case 31:
        if (m_backend.mutationInFlight() || m_backend.loading()) return;
        if (!arrivalMembersMatch(QJsonObject::fromVariantMap(m_backend.conversation()),false,false)) {
            finish("Native Unhold did not clear only the two original held members."); return;
        }
        if (!click("delayConversationButton")) return;
        m_stage=32; break;
    case 32:
        if (m_backend.mutationInFlight() || m_backend.loading()) return;
        if (!arrivalMembersMatch(QJsonObject::fromVariantMap(m_backend.conversation()),true,false)) {
            finish("Native Delay did not hold the two reviewed original members."); return;
        }
        m_stage=33;
        arrivalCli({"mail","read",m_arrivalThread},[this](const auto &source) {
            if (!arrivalMembersMatch(source,true,false)) finish("Independent CLI did not observe the exact reviewed/held snapshot before Done.");
        });
        break;
    case 33:
        m_stage=34;
        arrivalCli({"accounts"},[this](const auto &result) {
            QJsonObject account;
            for (const auto &value:result["accounts"].toArray()) if (value.toObject()["id"].toString()==m_arrivalAccount) account=value.toObject();
            if (account.isEmpty() || account["starred_count"].toInteger()<2 || account["held_conversation_count"].toInteger()<1) {
                finish("Independent account baseline omits the held snapshot members."); return;
            }
            // The independently read baseline has exactly two reviewed/held
            // originals. Done removes those stars; one unread reply then arrives.
            for (const auto key:{"message_count","unread_count","starred_count","conversation_count","unread_conversation_count","held_conversation_count","done_conversation_count"})
                m_arrivalExpectedCounts[key]=account[key];
            m_arrivalExpectedCounts["message_count"]=account["message_count"].toInteger()+1;
            m_arrivalExpectedCounts["unread_count"]=account["unread_count"].toInteger()+1;
            m_arrivalExpectedCounts["starred_count"]=account["starred_count"].toInteger()-m_arrivalOriginalIds.size();
            m_arrivalExpectedCounts["unread_conversation_count"]=account["unread_conversation_count"].toInteger()+1;
            m_arrivalExpectedCounts["held_conversation_count"]=account["held_conversation_count"].toInteger()-1;
        });
        break;
    case 34:
        if (m_backend.mutationInFlight() || m_backend.loading()) return;
        if (m_backend.selectedId()!=m_arrivalThread || draftFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_arrivalDraft) {
            finish("Native Delay or account readback changed the selected reader or authored draft."); return;
        }
        m_arrivalDoneArmed=true;
        if (!click("doneConversationButton")) { m_arrivalDoneArmed=false; return; }
        m_stage=35; break;
    case 35: {
        if (!m_arrivalReceiptSeen || m_backend.mutationInFlight() || m_backend.loading()) return;
        const auto report=arrivalReport();
        if (report["status"].toString()=="failed" || !report["errors"].toArray().isEmpty()) { finish("Engine provider ingestion failed during the native snapshot action."); return; }
        if (report["status"].toString()!="complete") return;
        m_arrivalId=report["arrival_message_id"].toString();
        if (!m_arrivalReceiptOk || !arrivalMembersMatch(m_arrivalReceipt,false,false)
            || report["trigger_count"].toInt()!=1 || report["committed_batches"].toInt()!=1 || report["committed_messages"].toInt()!=1
            || report["conversation_id"].toString()!=m_arrivalThread || report["original_message_ids"].toArray()!=m_arrivalOriginalIds) {
            finish("Native Done did not retain its exact original-member receipt across one real provider arrival."); return;
        }
        if (m_backend.conversation()["messages"].toList().size()!=3) return;
        // On screen with the app in front, the arrival is read as opening
        // would have read it, by its own write; Done never touches it.
        if (!m_arrivalRead && QGuiApplication::applicationState()==Qt::ApplicationActive) {
            m_arrivalRead=true;
            m_arrivalExpectedCounts["unread_count"]=m_arrivalExpectedCounts["unread_count"].toInteger()-1;
            m_arrivalExpectedCounts["unread_conversation_count"]=m_arrivalExpectedCounts["unread_conversation_count"].toInteger()-1;
            // Read and without stars, the thread counts as Done.
            m_arrivalExpectedCounts["done_conversation_count"]=m_arrivalExpectedCounts["done_conversation_count"].toInteger()+1;
        }
        if (m_arrivalRead && (m_backend.mutationInFlight() || std::any_of(m_backend.conversation()["messages"].toList().cbegin(), m_backend.conversation()["messages"].toList().cend(),
            [](const QVariant &m) { return m.toMap()["unread"].toBool(); }))) return;
        if (m_backend.selectedId()!=m_arrivalThread || !arrivalMembersMatch(QJsonObject::fromVariantMap(m_backend.conversation()),false,true)
            || draftFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_arrivalDraft) {
            finish("The arriving member inherited Done or refresh changed the selected reader or authored draft."); return;
        }
        if (!arrivalCountsMatch()) return;
        m_stage=36; break;
    }
    case 36:
        m_stage=37;
        arrivalCli({"mail","read",m_arrivalThread},[this](const auto &source) {
            if (!arrivalMembersMatch(source,false,true)) finish("Independent CLI did not keep the arrival apart from Done: only original members became Done, and the arrival is unread unless it was read on screen.");
        });
        break;
    case 37:
        m_stage=38;
        arrivalCli({"accounts"},[this](const auto &result) {
            QJsonObject account;
            for (const auto &value:result["accounts"].toArray()) if (value.toObject()["id"].toString()==m_arrivalAccount) account=value.toObject();
            for (auto it=m_arrivalExpectedCounts.begin();it!=m_arrivalExpectedCounts.end();++it) if (account[it.key()]!=it.value()) {
                finish("Independent CLI snapshot-arrival count differs for "+it.key()+"."); return;
            }
        });
        break;
    case 38:
        m_stage=39;
        arrivalCli({"draft","show",m_arrivalThread},[this](const auto &draft) {
            if (draftFields(draft)!=m_arrivalDraft) finish("Snapshot arrival changed the independently stored Unicode draft or identity snapshot.");
        });
        break;
    case 39:
        if (m_backend.selectedId()!=m_arrivalThread || !arrivalCountsMatch()
            || draftFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_arrivalDraft) {
            finish("Independent readback or background polling lost the snapshot-arrival selection, counts or draft."); return;
        }
        if (!retainNativeRow(m_arrivalThread,false)) {
            finish("The idle-polling delegate-retention check lost the visible arrival row."); return;
        }
        for (const auto &value:m_backend.accounts()) {
            const auto id=value.toMap()["id"].toString();
            const auto tab=find(m_window->contentItem(),"accountTab_"+id);
            if (id!=m_arrivalAccount && fullyVisible(tab,m_window)) {
                m_retainedAccount=id; m_retainedAccountTab=tab;
                m_retainedAccountRect=tab->mapRectToScene(QRectF(0,0,tab->width(),tab->height()));
                break;
            }
        }
        if (!m_retainedAccountTab) { finish("The account retention check needs a visible second mailbox tab."); return; }
        m_accountsBeforeHold=m_backend.accounts();
        // The preceding arrival can be observed by the Done callback's explicit
        // refresh. Change shared state from a separate CLI process now, while
        // leaving the GUI idle: only ordinary background polling may converge.
        m_stage=40;
        arrivalCli({"mail","flag",m_arrivalThread,"--starred","true"},[this](const auto &result) {
            const auto members=result["messages"].toArray();
            if (result["id"].toString()!=m_arrivalThread || members.size()!=3) {
                finish("The independent idle-polling mutation targeted the wrong conversation."); return;
            }
            for (const auto &member:members) if (!member.toObject()["starred"].toBool()) {
                finish("The independent idle-polling mutation did not hold all three exact members."); return;
            }
        });
        break;
    case 40: {
        if (m_backend.loading()) return;
        QVariantMap row;
        for (const auto &value:m_backend.conversations()) if (value.toMap()["id"].toString()==m_arrivalThread) row=value.toMap();
        const auto members=m_backend.conversation()["messages"].toList();
        if (!row["starred"].toBool() || members.size()!=3) return;
        for (const auto &member:members) if (!member.toMap()["starred"].toBool()) return;
        if (m_backend.selectedId()!=m_arrivalThread || m_backend.query()!="Snapshot arrival verification"
            || draftFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_arrivalDraft) {
            finish("Idle polling changed the active query, selected reader or authored draft."); return;
        }
        if (const auto error=retainedNativeRowError(m_arrivalThread); !error.isEmpty()) { finish(error); return; }
        if (m_backend.accounts()==m_accountsBeforeHold) return;
        if (const auto error=retainedAccountTabError(false); !error.isEmpty()) { finish(error); return; }
        m_accountsDuringHold=m_backend.accounts();
        // Keep a real native press active while the existing independent CLI
        // cleanup changes the account payload through ordinary background IPC.
        m_accountPressPoint=m_retainedAccountRect.center();
        const QPointF global=m_window->mapToGlobal(m_accountPressPoint.toPoint());
        QMouseEvent move(QEvent::MouseMove,m_accountPressPoint,global,Qt::NoButton,Qt::NoButton,Qt::NoModifier);
        QMouseEvent press(QEvent::MouseButtonPress,m_accountPressPoint,global,Qt::LeftButton,Qt::LeftButton,Qt::NoModifier);
        m_accountPressHeld=true;
        QCoreApplication::sendEvent(m_window,&move); QCoreApplication::sendEvent(m_window,&press);
        if (const auto error=retainedAccountTabError(true); !error.isEmpty()) { finish(error); return; }
        if (m_backend.account()!=m_arrivalAccount) { finish("The account tab activated before native release."); return; }
        m_stage=41;
        arrivalCli({"mail","flag",m_arrivalThread,"--starred","false"},[this](const auto &result) {
            if (!arrivalMembersMatch(result,false,true)) finish("Independent idle-polling cleanup changed the arrival's unread state or source snapshot.");
        });
        break;
    }
    case 41: {
        if (m_backend.loading() || !arrivalCountsMatch()
            || !arrivalMembersMatch(QJsonObject::fromVariantMap(m_backend.conversation()),false,true)) return;
        QVariantMap row;
        for (const auto &value:m_backend.conversations()) if (value.toMap()["id"].toString()==m_arrivalThread) row=value.toMap();
        if (row.isEmpty() || row["starred"].toBool()) return;
        if (row["unread"].toBool()==m_arrivalRead || m_backend.selectedId()!=m_arrivalThread || m_backend.query()!="Snapshot arrival verification"
            || draftFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_arrivalDraft) {
            finish("Idle polling cleanup changed the arriving member's read state, active query, selection or draft."); return;
        }
        if (const auto error=retainedNativeRowError(m_arrivalThread); !error.isEmpty()) { finish(error); return; }
        if (m_backend.accounts()==m_accountsDuringHold) return;
        if (const auto error=retainedAccountTabError(true); !error.isEmpty()) { finish(error); return; }
        const QPointF global=m_window->mapToGlobal(m_accountPressPoint.toPoint());
        QMouseEvent release(QEvent::MouseButtonRelease,m_accountPressPoint,global,Qt::LeftButton,Qt::NoButton,Qt::NoModifier);
        m_accountPressHeld=false;
        QCoreApplication::sendEvent(m_window,&release);
        if (m_backend.account()!=m_retainedAccount) {
            finish("Releasing the retained account tab activated the wrong mailbox after the count refresh."); return;
        }
        m_stage=42;
        arrivalCli({"mail","read",m_arrivalThread},[this](const auto &result) {
            if (!arrivalMembersMatch(result,false,true)) finish("Final independent readback disagrees with idle polling's restored state.");
        });
        break;
    }
    case 42:
        if (!m_accountReturnRequested) {
            if (m_backend.loading() || m_backend.draftBusy() || m_backend.draft().isEmpty()) return;
            if (m_backend.account()!=m_retainedAccount || m_backend.currentAccount()["id"].toString()!=m_retainedAccount) {
                finish("The retained account click did not hydrate the requested mailbox."); return;
            }
            m_accountReturnRequested=true;
            m_backend.selectAccount(m_arrivalAccount);
            return;
        }
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.draft().isEmpty()
            || m_backend.account()!=m_arrivalAccount || m_backend.selectedId()!=m_arrivalThread) return;
        if (draftFields(QJsonObject::fromVariantMap(m_backend.draft()))!=m_arrivalDraft) {
            finish("The account refresh/click round trip changed the origin mailbox's durable authored draft."); return;
        }
        qInfo().noquote() << "Native snapshot arrival passed: real Done receipt targets the two original members; provider arrival stays unstarred and apart from Done (read only when shown with the app in front); exact GUI/CLI counts, selected reader and Unicode draft retained. Counts:"
            << QString::fromUtf8(QJsonDocument(m_arrivalExpectedCounts).toJson(QJsonDocument::Compact));
        qInfo() << "Idle native polling passed: separate CLI hold/unhold converged in the actual list and reader without GUI query, refresh or navigation; original counts, the arrival's read state and authored draft restored.";
        qInfo() << "Native mail delegate retained through independent CLI hold/unhold: identical item, current row fields and unchanged viewport.";
        qInfo() << "Native account tab retained across two actual account payload changes: unchanged item/geometry, native press survived refresh, original-center release opened the exact mailbox and normal return retained the origin draft.";
        restore(); m_stage=5; break;
    case 5: {
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.selectedId() != m_originalSelected || m_backend.draft().isEmpty()) return;
        if (m_backend.account() != m_originalAccount || m_backend.query() != m_originalQuery) { finish("Search cleanup lost the original mailbox context."); return; }
        for (const auto &key : {"body", "body_html", "signature_text", "attachment_ids"})
            if (m_backend.draft().value(key) != m_originalDraft.value(key)) { finish("Search navigation changed the original authored draft."); return; }
        // Observe the real clear-query restoration before explicit cleanup can
        // overwrite presentation state. Folder and filter changes since the
        // search cases legitimately reset the saved offset, so the restored
        // list must show whatever offset the mailbox saved, from its origin.
        const auto list = find(m_window->contentItem(), "conversationList");
        if (!list) return;
        const auto origin = list->property("originY").toDouble(), minimum = origin - list->property("topMargin").toDouble();
        const auto maximum = qMax(minimum, origin + list->property("contentHeight").toDouble() + list->property("bottomMargin").toDouble() - list->height());
        const auto expectedY = qBound(minimum,origin+m_backend.presentation().value("listY").toDouble(),maximum);
        if (qAbs(list->property("contentY").toDouble()-expectedY)>1) return;
        for (auto it = m_originalView.cbegin(); it != m_originalView.cend(); ++it) m_backend.remember(it.key(), it.value());
        for (const auto &key : {"selectedMember", "readerY", "listY", "expanded", "replyOpen"})
            if (!m_originalView.contains(key)) m_backend.remember(key, QString(key) == "selectedMember" ? QVariant(QString{})
                : QString(key) == "expanded" || QString(key) == "replyOpen" ? QVariant(false) : QVariant(0));
        QMetaObject::invokeMethod(find(m_window->contentItem(), "mailWorkspace"), "restorePosition");
        m_window->setProperty("workspace", m_originalWorkspace); m_window->setProperty("settingsOpen", m_originalSettings);
        m_stage = 6; break;
    }
    case 6:
        if (!m_surfacesDone) {
            m_window->setProperty("settingsOpen",false); m_window->setProperty("workspace","mail");
            m_backend.setFolder("inbox"); m_backend.setCategory({}); m_backend.applyFilters({});
            if (!typeQuery({})) { finish("Cannot clear the actual search field for counted groups."); return; }
            m_arrivalAccount=m_originalAccount;
            m_stage=60; break;
        }
        if (m_originalFocus && m_originalFocus->isVisible() && m_originalFocus->isEnabled()) m_originalFocus->forceActiveFocus();
        finish(); break;
    case 60: {
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.m_searchTimer.isActive()) return;
        const auto path=QDir(m_backend.m_profile).filePath("native-mail-group-counts.json");
        QFile file(path); const auto bytes=QJsonDocument(QJsonObject{{"folder","inbox"},{"grouped",true},{"limit",1}}).toJson();
        if (!file.open(QIODevice::WriteOnly) || file.write(bytes)!=bytes.size()) { finish("Cannot prepare independent bounded group-count query."); return; }
        file.close(); m_stage=61;
        arrivalCli({"call","conversations","--params-file",path},[this](const auto &result) {
            m_surfaceCounts=result["filtered_group_counts"].toObject();
            qint64 total=0;
            for (const auto group:{"review","held","done"}) total+=m_surfaceCounts[group].toInteger();
            if (m_surfaceCounts.size()!=3 || total!=result["filtered_conversation_count"].toInteger()
                || total<=result["conversations"].toArray().size() || result["conversations"].toArray().size()!=1)
                finish("Filtered group counts are not the complete result independently of the CLI's one-row page.");
        });
        break;
    }
    case 61: {
        if (m_backend.loading()) return;
        if (QJsonObject::fromVariantMap(m_backend.querySummary()["filtered_group_counts"].toMap())!=m_surfaceCounts
            || m_backend.conversations().isEmpty()) { finish("Native group totals disagree with the independent pre-pagination CLI counts."); return; }
        // Wait for the structural page to render before positioning this test
        // setup. ListView originY can change during its deferred layout.
        if (m_collapseSettledFrame < 0) { m_collapseSettledFrame=m_renderedFrames; m_window->update(); return; }
        if (m_renderedFrames <= m_collapseSettledFrame) return;
        const auto first=m_backend.conversations().first().toMap();
        m_surfaceGroup=first.value("display_group",first["group"]).toString(); m_surfaceFirstId=first["id"].toString();
        auto list=find(m_window->contentItem(),"conversationList");
        if (!list) return;
        // This is deliberate setup at the first group, not an assertion that
        // query results repaired their viewport (the earlier cases prove that).
        QMetaObject::invokeMethod(list,"forceLayout");
        QMetaObject::invokeMethod(list,"positionViewAtBeginning");
        m_backend.remember("listY",list->property("contentY").toDouble()-list->property("originY").toDouble());
        if (auto search=find(m_window->contentItem(),"searchField")) key(search,Qt::Key_Escape);
        m_stage=62; break;
    }
    case 62: {
        auto heading=find(m_window->contentItem(),"mailGroup_"+m_surfaceGroup);
        if (!heading || !textItem(heading,QString::number(m_surfaceCounts[m_surfaceGroup].toInteger()))) {
            finish("The native group heading does not display its full filtered count."); return;
        }
        m_expandedBeforeCollapse=m_backend.querySummary()["expanded_conversation_count"].toLongLong();
        if (!clickItem(heading)) {
            if (m_collapseSettledFrame != -99) {
                qInfo()<<"Counted heading not clickable"<<geometryDiagnostic(heading,m_window)<<"enabled"<<heading->isEnabled()<<"folder"<<m_backend.folder()<<"first row"<<m_backend.conversations().first();
                m_collapseSettledFrame=-99;
            }
            return;
        }
        m_collapseSettledFrame=-1;
        m_stage=63; break;
    }
    case 63: {
        if (m_backend.loading()) { m_collapseSettledFrame=-1; return; }
        // Query completion precedes Qt's creation/layout of replacement group
        // headers. Assert the actual delegates after that page has rendered.
        if (m_collapseSettledFrame<0) {
            m_collapseSettledFrame=m_renderedFrames; m_window->update(); return;
        }
        if (m_renderedFrames<=m_collapseSettledFrame) return;
        auto heading=find(m_window->contentItem(),"mailGroup_"+m_surfaceGroup);
        if (!heading || !heading->property("collapsed").toBool()
            || find(m_window->contentItem(),"conversationRow_"+m_surfaceFirstId)
            || !textItem(heading,QString::number(m_surfaceCounts[m_surfaceGroup].toInteger()))
            || m_backend.selectedId()!=m_originalSelected
            || draftFields(QJsonObject::fromVariantMap(m_backend.draft()))!=draftFields(QJsonObject::fromVariantMap(m_originalDraft))) {
            finish(QString("Collapsing a native group lost its count, kept its row visible or changed the reader/draft. "
                "heading=%1 collapsed=%2 rowVisible=%3 count=%4 reader=%5 draft=%6 searchPopup=%7 geometry=%8")
                .arg(heading != nullptr).arg(heading && heading->property("collapsed").toBool())
                .arg(find(m_window->contentItem(),"conversationRow_"+m_surfaceFirstId) != nullptr)
                .arg(heading && textItem(heading,QString::number(m_surfaceCounts[m_surfaceGroup].toInteger())))
                .arg(m_backend.selectedId()==m_originalSelected)
                .arg(draftFields(QJsonObject::fromVariantMap(m_backend.draft()))==draftFields(QJsonObject::fromVariantMap(m_originalDraft)))
                .arg(m_window->property("searchPopupOpen").toBool()).arg(geometryDiagnostic(heading,m_window))); return;
        }
        // Done rows in folded older months are never expanded, so the
        // collapsed group comes off what was expanded just before.
        qint64 expanded=0;
        if (m_surfaceGroup=="done") for (const auto group:{"review","held"}) expanded+=m_surfaceCounts[group].toInteger();
        else expanded=m_expandedBeforeCollapse-m_surfaceCounts[m_surfaceGroup].toInteger();
        if (m_backend.querySummary()["expanded_conversation_count"].toLongLong()!=expanded
            || QJsonObject::fromVariantMap(m_backend.querySummary()["filtered_group_counts"].toMap())!=m_surfaceCounts
            || (expanded>0 && m_backend.conversations().isEmpty())) {
            finish("Collapsed-group paging lost full counts or left later expanded groups inaccessible."); return;
        }
        for (const auto &row:m_backend.conversations()) if (row.toMap()["group"].toString()==m_surfaceGroup) {
            finish("A collapsed group still consumed actual conversation page slots."); return;
        }
        if (auto list=find(m_window->contentItem(),"conversationList")) list->setProperty("contentY",list->property("originY"));
        if (!clickItem(heading)) return;
        m_stage=64; break;
    }
    case 64:
        if (m_backend.loading() || !m_backend.collapsedGroups().isEmpty()) return;
        if (!find(m_window->contentItem(),"conversationRow_"+m_surfaceFirstId)) return;
        if (!typeQuery("Weekly service availability")) { finish("Cannot type the classified fixture's source subject."); return; }
        m_stage=65; break;
    case 65:
        if (m_backend.loading() || m_backend.m_searchTimer.isActive()) return;
        if (m_backend.conversations().size()!=1 || m_backend.conversations().first().toMap()["kind"].toString()!="automated"
            || m_backend.conversations().first().toMap()["id"].toString()!="windscribe-thread-005") {
            finish("The source-backed automated fixture was not projected as exactly one automated conversation."); return;
        }
        if (auto search=find(m_window->contentItem(),"searchField")) key(search,Qt::Key_Escape);
        if (!click("senderKind_people")) return;
        m_stage=66; break;
    case 66:
        if (m_backend.loading() || m_backend.filters()["kind"].toString()!="people") return;
        if (!m_backend.conversations().isEmpty() || m_backend.querySummary()["filtered_conversation_count"].toInt()!=0) {
            finish("The People toggle included the explicitly automated retained source."); return;
        }
        if (!click("senderKind_people")) return;
        m_stage=67; break;
    case 67:
        if (m_backend.loading() || m_backend.filters()["kind"].toString()=="people") return;
        if (m_backend.conversations().size()!=1) { finish("Clearing People did not restore the filtered source."); return; }
        if (!click("senderKind_automated")) return;
        m_stage=68; break;
    case 68:
        if (m_backend.loading() || m_backend.filters()["kind"].toString()!="automated") return;
        if (m_backend.conversations().size()!=1 || m_backend.conversations().first().toMap()["kind"].toString()!="automated") {
            finish("The Automated toggle lost the exact source classification."); return;
        }
        // Passive selection establishes an unreviewed source baseline. Only the
        // later actual card row click is allowed to review it.
        m_backend.selectConversation("windscribe-thread-005"); m_stage=69; break;
    case 69:
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.selectedId()!="windscribe-thread-005"
            || m_backend.conversation()["id"].toString()!="windscribe-thread-005") return;
        if (auto reader=find(m_window->contentItem(),"reader")) reader->setProperty("contentY",0);
        m_stage=70;
        arrivalCli({"mail","read","windscribe-thread-005"},[this](const auto &result) { m_senderBefore=result; });
        break;
    case 70:
        if (!hoverSender()) return;
        m_stage=71; break;
    case 71: {
        const auto card=m_backend.senderCard();
        if (!senderPopup() || card["loading"].toBool()) return;
        const auto rows=card["conversations"].toList();
        if (card["account"].toString()!=m_originalAccount || card["email"].toString()!="status@service.example"
            || card["conversation_count"].toInt()!=1 || rows.size()!=1 || rows.first().toMap()["id"].toString()!="windscribe-thread-005") {
            finish("Sender hover mixed accounts, display-name identities or recent-source rows."); return;
        }
        m_stage=72;
        arrivalCli({"mail","read","windscribe-thread-005"},[this](const auto &result) {
            if (result!=m_senderBefore) finish("Pure sender hover changed the independently stored source or review flags.");
        });
        break;
    }
    case 72:
        if (!senderPopup()) { m_stage=70; break; }
        if (!click("senderCardRow_windscribe-thread-005")) return;
        m_stage=73; break;
    case 73:
        if (m_backend.loading() || m_backend.mutationInFlight() || senderPopup()) return;
        m_stage=74;
        arrivalCli({"mail","read","windscribe-thread-005"},[this](const auto &result) {
            const auto members=result["messages"].toArray();
            if (result["id"].toString()!="windscribe-thread-005" || members.size()!=1
                || members.first().toObject()["id"].toString()!="windscribe-message-005-000"
                || members.first().toObject()["unread"].toBool()) finish("Explicit native sender-card Open did not review exactly the retained source.");
        });
        break;
    case 74:
        if (auto reader=find(m_window->contentItem(),"reader")) reader->setProperty("contentY",0);
        if (!hoverSender()) return;
        m_stage=75; break;
    case 75:
        if (!senderPopup() || m_backend.senderCard()["loading"].toBool()) return;
        if (!click("senderCardFilter")) return;
        m_stage=76; break;
    case 76:
        if (m_backend.loading() || m_backend.m_searchTimer.isActive() || senderPopup()) return;
        if (m_backend.query()!="status@service.example" || m_backend.folder()!="all"
            || m_backend.filters()["kind"].toString()!="automated"
            || m_backend.conversations().size()!=1 || m_backend.selectedId()!="windscribe-thread-005") {
            finish("Sender-card Filter lost the address query, All mail scope, existing kind or reader."); return;
        }
        if (!hoverSender()) return;
        m_stable=0; m_stage=77; break;
    case 77:
        if (!senderPopup()) {
            if (++m_stable>10) finish("Sender hover did not reopen its card after Filter. Sender geometry: "
                +geometryDiagnostic(find(m_window->contentItem(),"senderFilter_windscribe-message-005-000"),m_window)
                +"; card="+QString::fromUtf8(QJsonDocument(QJsonObject::fromVariantMap(m_backend.senderCard())).toJson(QJsonDocument::Compact)));
            return;
        }
        if (m_backend.senderCard()["loading"].toBool() || m_backend.draftBusy()) return;
        if (!click("senderCardWrite")) {
            finish("Sender-card Write is missing, disabled or clipped: "+geometryDiagnostic(find(m_window->contentItem(),"senderCardWrite"),m_window)); return;
        }
        m_stage=78; break;
    case 78: {
        if (m_backend.draftBusy() || m_backend.m_draftDirty || m_backend.activeDraftId().isEmpty() || m_backend.draft()["mode"].toString()!="new") return;
        const auto draft=m_backend.draft(); const auto recipients=draft["to_recipients"].toList();
        if (m_backend.account()!=m_originalAccount || recipients.size()!=1
            || recipients.first().toMap()["email"].toString()!="status@service.example") {
            finish("Sender-card Write lost its account or exact address recipient."); return;
        }
        m_senderDraftId=m_backend.activeDraftId(); m_stage=79;
        arrivalCli({"draft","show","--draft-id",m_senderDraftId},[this](const auto &result) {
            const auto recipients=result["to_recipients"].toArray();
            if (result["draft_id"].toString()!=m_senderDraftId || result["mode"].toString()!="new"
                || result["account"].toString()!=m_originalAccount || recipients.size()!=1
                || recipients.first().toObject()["email"].toString()!="status@service.example")
                finish("Independent CLI did not retain the native sender-card Write draft and recipient.");
        });
        break;
    }
    case 79:
        if (m_backend.draftBusy()) return;
        qInfo() << "Native Update2 mail passed: complete CLI group counts, collapse retention, source-backed People/Automated toggles, account-scoped sender hover without review, explicit Open, exact Filter and durable Write recipient.";
        // The counted-group check deliberately persisted a temporary origin.
        // Restore its independent saved mailbox position before the existing
        // cleanup oracle checks actual viewport restoration and authored fields.
        m_backend.remember("listY",m_originalView.value("listY",0));
        m_surfacesDone=true; restore(); m_stage=5; break;
    }
}
