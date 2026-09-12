#include "GuiMailNavigationSelfTest.h"
#include <QCoreApplication>
#include <QDebug>
#include <QDir>
#include <QImage>
#include <QInputMethodEvent>
#include <QKeyEvent>
#include <QMouseEvent>
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
    m_timer.start();
}
void GuiMailNavigationSelfTest::finish(const QString &error) {
    m_timer.stop();
    auto callback = std::move(m_finished); m_finished = {};
    callback(error); deleteLater();
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
    auto control = find(m_window->contentItem(), name);
    // Do not scroll from the test to conceal a missing native reveal path.
    if (!control || !control->isEnabled() || !fullyVisible(control, m_window)) return false;
    const auto position = control->mapToScene({control->width()/2, control->height()/2});
    const QPointF global = m_window->mapToGlobal(position.toPoint());
    QMouseEvent press(QEvent::MouseButtonPress, position, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &press); QCoreApplication::sendEvent(m_window, &release);
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
void GuiMailNavigationSelfTest::step() {
    if (++m_ticks > 250) {
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
        restore(); m_stage = 5; break;
    }
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
