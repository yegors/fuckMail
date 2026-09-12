#include "GuiMailNavigationSelfTest.h"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QImage>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QJsonArray>
#include <array>

namespace {
struct SearchCase { const char *query; const char *thread; const char *message; const char *body; int members; };
// Independent expected source identities from fixture 2, not a projection copied
// from the query under test. The final body is the eighteenth of 32 messages.
constexpr std::array<SearchCase, 4> searches{{
    {"tomasz@windscribe.example", "windscribe-thread-001", "windscribe-message-001-000", "Revised paywall copy is ready", 1},
    {"Aisha Rahman", "windscribe-thread-002", "windscribe-message-002-000", "3 of 4 recommend hire", 1},
    {"WireGuard relay rollout", "windscribe-thread-003", "windscribe-message-003-000", "Toronto is clean since Thursday", 1},
    {"longthread17", "windscribe-thread-004", "windscribe-message-004-017", "Member 18 of 32.", 32}
}};
QQuickItem *find(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name && parent->isVisible()) return parent;
    for (auto child : parent->childItems()) if (auto result = find(child, name)) return result;
    return nullptr;
}
QQuickItem *bodyItem(QQuickItem *parent, const QString &expected) {
    if (parent->isVisible() && parent->property("readOnly").toBool()
        && parent->property("text").toString().contains(expected)) return parent;
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
}

GuiMailNavigationSelfTest::GuiMailNavigationSelfTest(MailBackend &backend, QQuickWindow *window,
    std::function<void(const QString &)> finished, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_finished(std::move(finished)) {
    m_timer.setInterval(80);
    connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    connect(&m_backend, &MailBackend::loadingChanged, this, [this] {
        if (m_searchPending && m_backend.query() == m_expectedQuery && m_backend.loading()) m_loadingSeen = true;
    });
    // A real socket write precedes its response. Inspect only the actual active
    // query, not loadingChanged (which fires before request construction).
    connect(&m_backend.m_socket, &QLocalSocket::bytesWritten, this, [this] { holdInFlightResponse(); });
    m_timer.start();
}
void GuiMailNavigationSelfTest::finish(const QString &error) {
    if (!m_finished) return;
    m_timer.stop();
    m_holdArmed = false;
    if (m_responseHeld && m_heldDelegate) { auto delegate=std::move(m_heldDelegate); m_heldDelegate={}; delegate(m_heldResult,m_heldOk); }
    auto callback = std::move(m_finished); m_finished = {};
    deleteLater(); callback(error);
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
bool GuiMailNavigationSelfTest::clickItem(QQuickItem *control) {
    // Do not scroll from the test to conceal a missing native reveal path.
    if (!control || !control->isEnabled() || !fullyVisible(control, m_window)) return false;
    const auto position = control->mapToScene({control->width()/2, control->height()/2});
    const QPointF global = m_window->mapToGlobal(position.toPoint());
    QMouseEvent press(QEvent::MouseButtonPress, position, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &press); QCoreApplication::sendEvent(m_window, &release);
    return true;
}
bool GuiMailNavigationSelfTest::typeField(const QString &name,const QString &text) {
    auto field=find(m_window->contentItem(),name);
    if (!field || !field->isEnabled() || !fullyVisible(field,m_window)) return false;
    field->forceActiveFocus(); QMetaObject::invokeMethod(field,"selectAll");
    if (text.isEmpty()) key(field,Qt::Key_Backspace);
    else { QInputMethodEvent input; input.setCommitString(text); QCoreApplication::sendEvent(field,&input); }
    return field->property("text").toString()==text;
}
bool GuiMailNavigationSelfTest::choose(const QString &name,int index) {
    auto field=find(m_window->contentItem(),name);
    if (!field || !field->isEnabled() || !fullyVisible(field,m_window)) return false;
    field->forceActiveFocus();
    const int delta=index-field->property("currentIndex").toInt();
    for (int i=0;i<qAbs(delta);++i) key(field,delta>0?Qt::Key_Down:Qt::Key_Up);
    return field->property("currentIndex").toInt()==index;
}
QObject *GuiMailNavigationSelfTest::filterPopup() const {
    for (auto object:m_window->findChildren<QObject *>("mailFilterPopup")) if (object->property("opened").toBool()) return object;
    return nullptr;
}
bool GuiMailNavigationSelfTest::filterFeedback(const QString &message) const {
    const auto popup=filterPopup();
    return popup && popup->property("validationError").toString()==message
        && fullyVisible(textItem(m_window->contentItem(),message),m_window);
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
void GuiMailNavigationSelfTest::step() {
    if (++m_ticks > 400) {
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
        m_stage = 1; break;
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
        if (!click("mailFiltersButton")) return;
        m_stage=11; break;
    case 11:
        if (!filterPopup()) return;
        if (!typeField("filterSender",searches[0].query) || !choose("filterStatus",3) || !choose("filterAttachments",2)
            || !typeField("filterAfter","2026-02-30") || !typeField("filterBefore","2026-09-12") || !click("applyMailFilters")) return;
        m_stage=12; break;
    case 12:
        if (!filterFeedback("Enter real dates as YYYY-MM-DD, or leave them empty.") || !m_backend.filters().isEmpty()
            || m_backend.conversations()!=m_unfilteredRows || find(m_window->contentItem(),"filterAfter")->property("text").toString()!="2026-02-30"
            || find(m_window->contentItem(),"filterSender")->property("text").toString()!=searches[0].query) { finish("Invalid date closed the filter popover, lost input or changed active results."); return; }
        if (!typeField("filterAfter","2026-09-11") || !click("applyMailFilters")) return;
        m_expectedFilters={{"sender",QString(searches[0].query)},{"status","done"},{"has_attachment",false},{"after","2026-09-11"},{"before","2026-09-12"}};
        m_stage=13; break;
    case 13:
        if (filterPopup() || m_backend.loading()) return;
        if (m_backend.filters()!=m_expectedFilters || !filterResultMatches()) { finish("Native combined filters did not return the exact source member with the requested state and date bounds."); return; }
        if (!click("mailFiltersButton")) return;
        m_stage=14; break;
    case 14:
        if (!filterPopup()) return;
        if (find(m_window->contentItem(),"filterSender")->property("text").toString()!=searches[0].query
            || find(m_window->contentItem(),"filterBefore")->property("text").toString()!="2026-09-12") { finish("Reopening filters lost the saved criteria."); return; }
        if (!typeField("filterSender",QString(QChar(0x00e9)).repeated(129)) || !click("applyMailFilters")) return;
        m_stage=15; break;
    case 15:
        if (!filterFeedback("Use a shorter sender name or email without control characters.") || m_backend.filters()!=m_expectedFilters || !filterResultMatches()
            || find(m_window->contentItem(),"filterSender")->property("text").toString()!=QString(QChar(0x00e9)).repeated(129)) { finish("Overlong UTF-8 sender input was not retained with the old active filters."); return; }
        if (!typeField("filterSender",searches[0].query) || !typeField("filterBefore","2026-09-11") || !click("applyMailFilters")) return;
        m_stage=16; break;
    case 16:
        if (!filterFeedback("The before date must be later than the on or after date.") || m_backend.filters()!=m_expectedFilters || !filterResultMatches()
            || find(m_window->contentItem(),"filterBefore")->property("text").toString()!="2026-09-11") { finish("Inverted date bounds changed active filters or lost correction input."); return; }
        if (!typeField("filterBefore","2026-09-12") || !click("applyMailFilters")) return;
        m_stage=17; break;
    case 17:
        if (filterPopup() || m_backend.loading()) return;
        if (m_backend.filters()!=m_expectedFilters || !filterResultMatches()) { finish("Corrected filter input did not restore the valid criteria."); return; }
        if (!click("mailFiltersButton")) return;
        m_stage=18; break;
    case 18:
        if (!filterPopup() || !click("clearMailFilters")) return;
        m_stage=19; break;
    case 19:
        if (filterPopup() || m_backend.loading()) return;
        if (!m_backend.filters().isEmpty() || m_backend.conversations()!=m_unfilteredRows || !m_backend.query().isEmpty()
            || m_backend.account()!=m_originalAccount || m_backend.selectedId()!=searches[3].thread) { finish("Clearing native filters did not restore the unfiltered mailbox context and retained selection."); return; }
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
        qInfo() << "Native mail filters passed valid/date/UTF-8 error/correction/clear controls. Real response" << m_heldRequestId
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
        restore(); m_stage=5; break;
    case 5: {
        if (m_backend.loading() || m_backend.draftBusy() || m_backend.selectedId() != m_originalSelected || m_backend.draft().isEmpty()) return;
        if (m_backend.account() != m_originalAccount || m_backend.query() != m_originalQuery) { finish("Search cleanup lost the original mailbox context."); return; }
        for (const auto &key : {"body", "body_html", "signature_text", "attachment_ids"})
            if (m_backend.draft().value(key) != m_originalDraft.value(key)) { finish("Search navigation changed the original authored draft."); return; }
        // Observe the real clear-query restoration before explicit cleanup can
        // overwrite presentation state and hide a lost mailbox scroll position.
        const auto list = find(m_window->contentItem(), "conversationList");
        if (!list) return;
        const auto minimum = list->property("originY").toDouble();
        const auto maximum = minimum + qMax(0.0,list->property("contentHeight").toDouble()-list->height());
        const auto expectedY = qBound(minimum,m_originalView["listY"].toDouble(),maximum);
        if (qAbs(list->property("contentY").toDouble()-expectedY)>1) return;
        if (m_backend.presentation().value("listY") != m_originalView.value("listY")) {
            finish("Search result clamping overwrote the saved mailbox scroll position."); return;
        }
        for (auto it = m_originalView.cbegin(); it != m_originalView.cend(); ++it) m_backend.remember(it.key(), it.value());
        for (const auto &key : {"selectedMember", "readerY", "listY", "expanded", "replyOpen"})
            if (!m_originalView.contains(key)) m_backend.remember(key, QString(key) == "selectedMember" ? QVariant(QString{})
                : QString(key) == "expanded" || QString(key) == "replyOpen" ? QVariant(false) : QVariant(0));
        QMetaObject::invokeMethod(find(m_window->contentItem(), "mailWorkspace"), "restorePosition");
        m_window->setProperty("workspace", m_originalWorkspace); m_window->setProperty("settingsOpen", m_originalSettings);
        m_stage = 6; break;
    }
    case 6:
        if (m_originalFocus && m_originalFocus->isVisible() && m_originalFocus->isEnabled()) m_originalFocus->forceActiveFocus();
        finish(); break;
    }
}
