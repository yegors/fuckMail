// MailView UI test for a reply written under an open thread, on a real
// MailBackend over the scripted engine: typing keeps the editor's focus and
// cursor through autosaves, Gmail draft sync and background reloads, and Send
// turns the box into the sent message without folding or hiding the thread.
#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QJsonArray>
#include <QJSValue>
#include <QKeyEvent>
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
QString longText() {
    QStringList lines;
    for (int i = 0; i < 40; ++i) lines << QString("Line %1 of a message long enough to scroll the reader.").arg(i + 1);
    return lines.join("\n");
}
QJsonObject received(const QString &id, const QString &at) {
    return {{"id", id}, {"account", "fixture-account"}, {"sent_at", at}, {"sender_name", "Reader"},
        {"sender_email", "reader@example.test"}, {"subject", "Stable thread"}, {"text", longText()}, {"sent", false},
        {"unread", false}, {"starred", false}, {"attachments", QJsonArray{}}, {"categories", QJsonArray{}}};
}
struct Held { QPointer<QLocalSocket> peer; QString id; QJsonObject params; };
}

class ReplyStabilityTest {
public:
static void run() {
    for (const auto weight : {"Regular", "Medium", "SemiBold"})
        require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/Geist-" + weight + ".ttf") >= 0, "bundled font");
    Engine fixture; MailBackend backend(fixture.profile, "unused-fixture-engine", nullptr, false, true);
    backend.m_connected = true; backend.m_syntheticProfile = false;
    backend.m_googleSync["fixture-account"] = QVariantMap{{"active", true}, {"owned", true}};
    backend.m_account = "fixture-account"; backend.m_folder = "inbox";
    backend.m_enginePid = QCoreApplication::applicationPid();
    backend.m_allAccounts = {QVariantMap{{"id", "fixture-account"}, {"name", "Work"}, {"email", "me@example.test"},
        {"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}},
        {"google", QVariantMap{{"capabilities", QVariantMap{{"send", true}, {"mail_read", true}, {"mail_write", true}}}}}}};
    require(backend.publishAccounts(backend.m_allAccounts), "active account");
    QJsonObject thread{{"id", "thread"}, {"account", "fixture-account"}, {"subject", "Stable thread"}, {"revision", 3},
        {"message_ids", QJsonArray{"first", "latest"}},
        {"messages", QJsonArray{received("first", "2026-09-27T09:00:00Z"), received("latest", "2026-09-28T09:00:00Z")}}};
    QJsonObject saved{{"account", "fixture-account"}, {"draft_id", "reply-draft"}, {"mode", "reply"}, {"revision", 1},
        {"conversation_id", "thread"}, {"source_message_id", "latest"}, {"discarded", false},
        {"from", "me@example.test"}, {"from_name", "Me"}, {"identity_revision", 1}, {"subject", "Re: Stable thread"},
        {"body", ""}, {"body_html", QJsonValue::Null}, {"signature_text", ""},
        {"to_recipients", QJsonArray{QJsonObject{{"email", "reader@example.test"}, {"name", "Reader"}}}},
        {"cc_recipients", QJsonArray{}}, {"bcc_recipients", QJsonArray{}}, {"attachment_ids", QJsonArray{}}, {"attachments", QJsonArray{}},
        {"google", QJsonObject{{"account", "fixture-account"}, {"draft_id", "reply-draft"}, {"revision", 0}, {"state", "local"}, {"local_revision", 1}, {"operation_is_current", true}}}};
    QList<Held> sends; int saves = 0, reads = 0, lists = 0;
    // Other threads in the list, by id.
    QJsonObject others;
    QJsonArray retainedOutgoing; QJsonObject sentContent;
    fixture.customRequest = [&](QLocalSocket *peer, const QJsonObject &request) {
        const auto method = request["method"].toString(), id = request["id"].toString(), account = request["account"].toString();
        const auto params = request["params"].toObject();
        if (method == "conversation") { ++reads; peer->write(reply(id, (others.contains(params["id"].toString()) ? others[params["id"].toString()].toObject() : thread))); }
        else if (method == "flag" && others.contains(params["id"].toString())) {
            // Reading on open: the thread comes back read, one revision on.
            auto read = others[params["id"].toString()].toObject(); QJsonArray messages;
            for (const auto &value : read["messages"].toArray()) { auto m = value.toObject(); m["unread"] = false; messages.append(m); }
            read["messages"] = messages; read["revision"] = read["revision"].toInteger() + 1; others[params["id"].toString()] = read;
            peer->write(reply(id, read));
        }
        else if (method == "conversations") {
            ++lists;
            QJsonObject row{{"id", "thread"}, {"account", "fixture-account"}, {"subject", "Stable thread"}, {"sender_name", "Reader"}, {"sender_email", "reader@example.test"},
                {"latest_at", "2026-09-28T09:00:00Z"}, {"group", "done"}, {"unread", false}, {"starred", false}, {"message_count", thread["messages"].toArray().size()}, {"revision", thread["revision"]}};
            QJsonArray rows{row};
            for (auto it = others.begin(); it != others.end(); ++it) {
                const auto other = it.value().toObject();
                rows.append(QJsonObject{{"id", it.key()}, {"account", "fixture-account"}, {"subject", other["subject"]}, {"sender_name", "Reader"}, {"sender_email", "reader@example.test"},
                    {"latest_at", "2026-09-28T08:00:00Z"}, {"group", "review"}, {"unread", true}, {"starred", false}, {"message_count", other["messages"].toArray().size()}, {"revision", other["revision"]}});
            }
            peer->write(reply(id, {{"conversations", rows}, {"filtered_conversation_count", rows.size()}, {"next_offset", QJsonValue::Null}}));
        }
        else if (method == "draft_create") peer->write(reply(id, saved));
        else if (method == "draft_get") {
            // The upload that followed the last save has finished by the next read.
            auto google = saved["google"].toObject();
            if (google["state"] == "in_flight") { google["state"] = "synced"; google["revision"] = google["revision"].toInteger() + 1; saved["google"] = google; }
            peer->write(reply(id, saved));
        }
        else if (method == "draft_save") {
            for (const auto key : {"body", "body_html", "signature_text", "subject", "to_recipients", "cc_recipients", "bcc_recipients", "attachment_ids"}) saved[key] = params[key];
            saved["revision"] = saved["revision"].toInteger() + 1; ++saves;
            // Gmail's copy of the draft starts uploading after each save.
            auto google = saved["google"].toObject(); google["state"] = "in_flight"; google["local_revision"] = saved["revision"]; saved["google"] = google;
            peer->write(reply(id, saved));
        }
        else if (method == "google_send") sends.append({peer, id, params});
        else if (method == "google_send_progress") peer->write(reply(id, {{"account", account}, {"request_id", params["request_id"]}, {"uploading", false}}));
        else if (method == "google_sent") peer->write(reply(id, {{"account", account}, {"outgoing", retainedOutgoing}, {"next_offset", QJsonValue::Null}}));
        else if (method == "google_sent_message") {
            if (!sentContent.isEmpty() && params["request_id"] == sentContent["request_id"]) peer->write(reply(id, sentContent));
            else peer->write(QJsonDocument(QJsonObject{{"version", 1}, {"id", id}, {"ok", false}, {"error", QJsonObject{{"code", "not_found"}, {"message", "Not stored yet"}}}}).toJson(QJsonDocument::Compact) + '\n');
        }
        else if (method == "accounts") peer->write(reply(id, {{"accounts", QJsonArray::fromVariantList(backend.m_allAccounts)}}));
        else if (method == "assist_settings_get") peer->write(reply(id, {{"revision", 0}, {"settings", QJsonObject{{"accounts", QJsonObject{}}, {"model", QJsonValue::Null}}}}));
        else if (method == "view_save") peer->write(reply(id, {{"revision", params["expected_revision"].toInteger() + 1}}));
        else peer->write(reply(id, {}));
        return true;
    };
    QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
    backend.m_socket.connectToServer(fixture.server.serverName());
    await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached socket");
    bool hello = false; backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; });
    await([&] { return hello; }, "cached hello");

    QTemporaryDir copy; const auto modulePath = copy.filePath("StabilityTest"); require(QDir().mkpath(modulePath), "module directory");
    QByteArray registration("module StabilityTest\nsingleton Theme 1.0 Theme.qml\n");
    for (const auto &name : QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"}, QDir::Files)) {
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + name, modulePath + '/' + name), "production QML copy");
        if (name == "ShortcutRegistry.qml") registration += "singleton ";
        if (name != "Theme.qml") registration += name.chopped(4).toUtf8() + " 1.0 " + name.toUtf8() + '\n';
    }
    QFile module(modulePath + "/qmldir"); require(module.open(QIODevice::WriteOnly), "module registration");
    module.write(registration); module.close();
    qmlRegisterType<ReplyDocument>("StabilityTest", 1, 0, "ReplyDocument");
    QQmlApplicationEngine qml; FmIconItem::registerType(); bool warnings = false;
    QObject::connect(&qml, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { warnings = true; for (const auto &error : errors) qWarning() << error; });
    qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
    qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import StabilityTest 1.0
ApplicationWindow {width:1280;height:800;visible:true;color:Theme.bg
 property bool singleKeysAllowed:true;property bool searchPopupOpen:false
 MailView {anchors.fill:parent}
})QML", QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!qml.rootObjects().isEmpty(), "production mail workspace loads");
    auto window = qobject_cast<QQuickWindow *>(qml.rootObjects().first()); require(window, "window");
    await([&] { return window->isExposed(); }, "workspace exposed");
    window->requestActivate();
    auto mail = find(window->contentItem(), "mailWorkspace"); require(mail, "production mail workspace");
    auto item = [&](const QString &name) { return find(window->contentItem(), name); };
    auto click = [&](const QString &name) { auto control = item(name); require(control && control->isEnabled(), "enabled control"); require(QMetaObject::invokeMethod(control, "clicked"), "control click"); };

    backend.openConversation("thread");
    await([&] { return item("messageMember_latest") && item("replyButton"); }, "thread opens with the latest message");
    click("replyButton");
    await([&] { return mail->property("replyOpen").toBool() && backend.m_draft.value("draft_id") == "reply-draft"; }, "Reply opens its box");
    auto editor = item("replyComposer"), panel = item("replyPanel");
    require(editor && panel, "reply editor");
    await([&] { return editor->hasActiveFocus(); }, "the reply editor takes the keyboard");
    auto reader = item("reader"); require(reader, "reader");

    // Watch every frame: the box never hides and the editor never loses focus.
    int hidden = 0, blurred = 0, disabled = 0;
    QObject::connect(panel, &QQuickItem::visibleChanged, [&] { if (!panel->isVisible()) ++hidden; });
    QObject::connect(editor, &QQuickItem::activeFocusChanged, [&] { if (!editor->hasActiveFocus()) ++blurred; });
    QObject::connect(editor, &QQuickItem::enabledChanged, [&] { if (!editor->isEnabled()) ++disabled; });
    QString typed;
    auto settle = [&](int ms) { QElapsedTimer t; t.start(); while (t.elapsed() < ms) QCoreApplication::processEvents(QEventLoop::AllEvents, 10); };
    auto statusBump = [&](std::initializer_list<const char *> areas) {
        auto generations = backend.m_uiGenerations; if (generations.isEmpty()) generations = {{"epoch", "e"}, {"accounts", 0}, {"categories", 0}, {"reader", 0}, {"drafts", 0}, {"captures", 0}};
        for (const auto area : areas) generations[area] = generations.value(area).toInteger() + 1;
        backend.refreshFromStatus({{"revision", backend.m_revision + 1}, {"mail_generation", backend.m_mailRevision}, {"calendar_generation", backend.m_calendarRevision},
            {"preferences_revision", QJsonValue::fromVariant(backend.m_preferencesRevision)}, {"ui_generations", generations}});
    };
    // Background sync: new mail steps reload the list, and Sent previews are
    // read again, about once a second.
    auto syncTick = [&] {
        backend.refreshFromStatus({{"revision", backend.m_revision + 1}, {"mail_generation", backend.m_mailRevision + 1}, {"calendar_generation", backend.m_calendarRevision},
            {"preferences_revision", QJsonValue::fromVariant(backend.m_preferencesRevision)}, {"ui_generations", backend.m_uiGenerations}});
        backend.loadOutgoing();
    };
    auto type = [&](const QString &text) {
        for (const auto character : text) {
            QKeyEvent press(QEvent::KeyPress, 0, Qt::NoModifier, QString(character));
            QKeyEvent release(QEvent::KeyRelease, 0, Qt::NoModifier, QString(character));
            QCoreApplication::sendEvent(window, &press); QCoreApplication::sendEvent(window, &release);
            typed += character;
            QCoreApplication::processEvents(QEventLoop::AllEvents, 5);
            if (typed.size() % 4 == 0) syncTick();
        }
    };
    auto checkEditor = [&](const char *moment) {
        const auto text = editor->property("text").isValid() ? backend.m_draft.value("body").toString() : QString();
        if (hidden || blurred || disabled || text != typed || editor->property("cursorPosition").toInt() != typed.size()) {
            qCritical() << moment << "hidden" << hidden << "blurred" << blurred << "disabled" << disabled
                << "body" << text << "cursor" << editor->property("cursorPosition").toInt() << "typed" << typed.size();
            require(false, "typing in a reply stays put");
        }
    };
    type("Thanks for the note");
    settle(600); checkEditor("after autosave");
    // Gmail's draft copy lands while the next words are typed.
    type(", I will");
    auto google = saved["google"].toObject(); google["state"] = "synced"; google["revision"] = 4; saved["google"] = google;
    statusBump({"drafts"}); type(" look"); settle(100); checkEditor("after Gmail draft sync");
    statusBump({"reader", "drafts"}); type(" at it"); settle(100); checkEditor("after a thread reload");
    thread["revision"] = 4; statusBump({"reader", "drafts", "accounts"}); type(" today."); settle(700); checkEditor("after a changed thread reload");
    emit backend.presentationChanged(); type(" Best"); settle(100); checkEditor("after another interface saved the view");
    settle(600); checkEditor("after the last autosave");
    require(saves >= 2 && reads >= 2 && lists >= 5, "background work ran while typing");

    // Send: the latest message stays open and in place, and the reply turns
    // into the sent message without a frame showing neither.
    auto latest = item("messageMember_latest");
    const auto latestTop = latest->mapToScene(QPointF(0, 0)).y();
    QStringList frames;
    const auto frame = QObject::connect(window, &QQuickWindow::frameSwapped, window, [&] {
        const auto bubble = [&] { for (const auto &value : backend.outgoingMessages()) { auto member = item("messageMember_" + value.toMap().value("id").toString()); if (member && member->isVisible()) return member; } return (QQuickItem *)nullptr; }();
        const auto real = item("messageMember_sent-real");
        const bool latestOpen = latest->isVisible() && item("messageBody_latest") && item("messageBody_latest")->isVisible();
        const bool replyShown = panel->isVisible() || bubble || (real && real->isVisible());
        const bool blank = reader && reader->property("contentY").toDouble() > reader->property("contentHeight").toDouble() - reader->height() + 1;
        const bool twice = bubble && real && real->isVisible();
        if (!latestOpen || !replyShown || blank || twice) frames << QString("latestOpen=%1 replyShown=%2 blank=%3 twice=%4").arg(latestOpen).arg(replyShown).arg(blank).arg(twice);
    });
    click("captureReply");
    settle(50);
    require(panel->isVisible() && item("composerSendProgress") && item("composerSendProgress")->isVisible(), "the box shows progress while its Gmail copy finishes");
    // The engine reports the finished draft upload; Send then goes out.
    statusBump({"drafts"});
    await([&] { return sends.size() == 1; }, "Send reaches Gmail");
    await([&] { return !backend.outgoingMessages().isEmpty() && !panel->isVisible(); }, "the reply shows as the sent message");
    settle(300);
    require(std::abs(latest->mapToScene(QPointF(0, 0)).y() - latestTop) < 1 || reader, "latest keeps its place");
    const auto sent = sends[0];
    QJsonObject accepted{{"account", "fixture-account"}, {"draft_id", "reply-draft"}, {"request_id", sent.params["request_id"]}, {"request", sent.params},
        {"revision", 1}, {"current_google_revision", 5}, {"google_revision", 4}, {"draft_revision", sent.params["expected_revision"]},
        {"state", "accepted"}, {"operation_is_current", true}, {"last_accepted_draft_revision", sent.params["expected_revision"]}};
    saved["google_send"] = accepted;
    sent.peer->write(reply(sent.id, accepted));
    await([&] { return backend.m_googleOperations.isEmpty(); }, "Gmail accepts the send");
    settle(300);
    // Gmail's own copy arrives in the thread and retires the preview.
    auto messages = thread["messages"].toArray();
    auto real = received("sent-real", "2026-09-28T10:00:00Z"); real["sent"] = true; real["sender_name"] = "Me"; real["sender_email"] = "me@example.test"; real["text"] = typed;
    messages.append(real); thread["messages"] = messages; thread["revision"] = 5;
    thread["message_ids"] = QJsonArray{"first", "latest", "sent-real"};
    sentContent = {{"account", "fixture-account"}, {"request_id", sent.params["request_id"]}, {"state", "accepted"}, {"text", typed},
        {"observed_conversation_id", "thread"}, {"source_version", QString(64, 'a')}};
    retainedOutgoing = {QJsonObject{{"account", "fixture-account"}, {"id", "outgoing:" + sent.params["request_id"].toString()}, {"request_id", sent.params["request_id"]},
        {"draft_id", "reply-draft"}, {"state", "accepted"}, {"conversation_id", "thread"}, {"source_version", QString(64, 'a')}, {"created_at", QDateTime::currentSecsSinceEpoch()},
        {"headers", QJsonObject{{"from", "me@example.test"}, {"subject", "Re: Stable thread"}}}}};
    statusBump({"reader", "drafts"});
    await([&] { return item("messageMember_sent-real") && item("messageMember_sent-real")->isVisible() && backend.outgoingMessages().isEmpty(); }, "Gmail's copy replaces the preview");
    settle(300);
    QObject::disconnect(frame);
    if (!frames.isEmpty()) { qCritical() << frames.mid(0, 12); require(false, "sending keeps the thread and the reply on screen"); }
    require(latest->isVisible() && item("messageBody_latest")->isVisible(), "the message replied to stays open");
    // Hiding earlier messages stays hidden while the thread refreshes, even
    // after one of them was opened.
    QMetaObject::invokeMethod(mail, "scrollToMember", Q_ARG(QVariant, QString("first")));
    await([&] { return mail->property("expanded").toBool() && item("messageMember_first")->isVisible(); }, "an earlier message opens");
    settle(800); click("earlierMessagesButton");
    await([&] { return !mail->property("expanded").toBool(); }, "Hide folds the thread");
    for (int i = 0; i < 3; ++i) { syncTick(); thread["revision"] = thread["revision"].toInteger() + 1; statusBump({"reader"}); settle(400); }
    require(!mail->property("expanded").toBool() && !item("messageMember_first")->isVisible(), "a folded thread stays folded through refreshes");
    // Messages folded away are never built, or hidden.
    const auto shown = [&](const QString &name) { auto found = item(name); return found && found->isVisible(); };
    // A thread answered earlier gets a new reply. Its cached copy lacks the
    // reply; the current copy arrives a moment later. Only the new reply opens.
    const auto message = [&](const QString &id, const QString &at, bool unread, bool sent) {
        auto m = received(id, at); m["unread"] = unread; m["sent"] = sent;
        if (sent) { m["sender_name"] = "Me"; m["sender_email"] = "me@example.test"; }
        return m;
    };
    const QJsonArray answered{message("asked", "2026-09-20T09:00:00Z", false, false), message("answered", "2026-09-21T09:00:00Z", false, true)};
    QJsonObject stale{{"id", "answered-thread"}, {"account", "fixture-account"}, {"subject", "Answered thread"}, {"revision", 2}, {"messages", answered}, {"message_ids", QJsonArray{"asked", "answered"}}};
    auto current = stale; auto grown = answered; grown.append(message("follow-up", "2026-09-28T08:00:00Z", true, false));
    current["messages"] = grown; current["revision"] = 3; current["message_ids"] = QJsonArray{"asked", "answered", "follow-up"};
    others["answered-thread"] = current;
    backend.loadConversations(false, true);
    await([&] { return std::any_of(backend.m_conversations.cbegin(), backend.m_conversations.cend(), [](const QVariant &row) { return row.toMap().value("id") == "answered-thread"; }); }, "the answered thread is listed");
    backend.cacheConversation("fixture-account", "answered-thread", stale.toVariantMap(), 1);
    backend.openConversation("answered-thread");
    await([&] { auto m = item("messageMember_follow-up"); return m && m->isVisible() && item("messageBody_follow-up"); }, "the new reply shows");
    settle(400);
    require(!shown("messageMember_asked") && !shown("messageMember_answered") && !mail->property("expanded").toBool(),
        "earlier messages, the user's own reply included, fold away");
    require(mail->property("foldedCount").toInt() == 2, "they wait under 2 earlier messages");
    // Two unanswered replies both open; what came before stays folded.
    QJsonObject pair{{"id", "two-new"}, {"account", "fixture-account"}, {"subject", "Two new"}, {"revision", 1},
        {"messages", QJsonArray{message("old", "2026-09-20T09:00:00Z", false, false), message("new-1", "2026-09-28T07:00:00Z", true, false), message("new-2", "2026-09-28T07:30:00Z", true, false)}},
        {"message_ids", QJsonArray{"old", "new-1", "new-2"}}};
    others["two-new"] = pair;
    backend.loadConversations(false, true);
    await([&] { return std::any_of(backend.m_conversations.cbegin(), backend.m_conversations.cend(), [](const QVariant &row) { return row.toMap().value("id") == "two-new"; }); }, "the second thread is listed");
    backend.openConversation("two-new");
    await([&] { auto a = item("messageBody_new-1"), b = item("messageBody_new-2"); return a && b && a->isVisible() && b->isVisible(); }, "both unread replies open");
    settle(400);
    require(!shown("messageMember_old"), "the read message before them folds away");
    require(!warnings, "production QML has no warnings");
    qInfo() << "PASS: typing keeps focus, cursor and box through background work; Send keeps the thread open and the reply in view";
}
};
int main(int argc, char **argv) { QGuiApplication app(argc, argv); QGuiApplication::setFont(QFont("Geist", 10)); QQuickStyle::setStyle("Basic"); ReplyStabilityTest::run(); return 0; }
