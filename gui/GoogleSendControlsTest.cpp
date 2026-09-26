// ReplyComposer UI test for Send, on a real MailBackend over the scripted engine:
// recipient validation, save before send, resume and abandon of an unconfirmed
// send, historical receipts, and isolation between accounts and drafts.
#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QFontInfo>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
#include <utility>

using namespace GoogleOperationFixture;
namespace {
QQuickItem *visualNamed(QQuickItem *item, const QString &name) {
    if (item->objectName() == name) return item;
    for (auto child : item->childItems()) if (auto found = visualNamed(child, name)) return found;
    return nullptr;
}
QObject *named(QObject *root, const char *name) {
    if (auto found = root->findChild<QObject *>(QString::fromUtf8(name))) return found;
    if (auto window = qobject_cast<QQuickWindow *>(root)) return visualNamed(window->contentItem(), QString::fromUtf8(name));
    return nullptr;
}
QVariantMap authored(QVariantMap draft) { draft.remove("google"); draft.remove("google_send"); return draft; }
QJsonObject projection(qint64 revision = 5) {
    return {{"account", "fixture-account"}, {"draft_id", "draft-fixture"}, {"conversation_id", QJsonValue::Null},
        {"mode", "new"}, {"revision", revision}, {"discarded", false}, {"from", "me@example.test"}, {"from_name", "Fixture author"},
        {"identity_revision", 1}, {"subject", "Human subject 日本語"}, {"body", "Exact authored body café"}, {"body_html", QJsonValue::Null},
        {"signature_text", "Signature"}, {"attachment_ids", QJsonArray{}}, {"attachments", QJsonArray{}},
        {"to_recipients", QJsonArray{QJsonObject{{"email", "reader@example.test"}, {"name", "Reader"}}}},
        {"cc_recipients", QJsonArray{}}, {"bcc_recipients", QJsonArray{}},
        {"google", QJsonObject{{"account", "fixture-account"}, {"draft_id", "draft-fixture"}, {"state", "local"},
            {"revision", 0}, {"local_revision", revision}, {"operation_is_current", true}}}, {"google_send", QJsonValue::Null}};
}
QJsonObject status(const QJsonObject &request, const QString &state, qint64 revision, qint64 watermark, qint64 accepted = 0) {
    return {{"account", "fixture-account"}, {"draft_id", request["draft_id"]}, {"request_id", request["request_id"]},
        {"revision", revision}, {"current_google_revision", watermark}, {"state", state}, {"draft_revision", request["expected_revision"]},
        {"google_revision", request["expected_google_revision"]}, {"operation_is_current", true}, {"request", request},
        {"last_accepted_draft_revision", accepted > 0 ? QJsonValue(accepted) : QJsonValue::Null}, {"read_evidence", QJsonValue::Null},
        {"ack", state == "accepted" ? QJsonValue(QJsonObject{{"message_id", "sent-message"}, {"thread_id", "sent-thread"}}) : QJsonValue::Null}};
}
QByteArray refusal(const QString &id) {
    return QJsonDocument(QJsonObject{{"version", 1}, {"id", id}, {"ok", false},
        {"error", QJsonObject{{"code", "local_io"}, {"message", "Fixture save refused"}}}}).toJson(QJsonDocument::Compact) + '\n';
}
}

class GoogleSendControlsTest {
public:
    static void run() {
        for (const auto weight : {"Regular", "Medium", "SemiBold"})
            require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR)
                + "/fonts/Geist-" + weight + ".ttf") >= 0, "load the same bundled Geist fonts as the app");
        Engine fixture;
        MailBackend backend(fixture.profile, "unused-fixture-engine", nullptr, false, true);
        backend.m_sendUndoMs = -1; // Isolate operation controls; SendUndoTest covers save/preflight/dispatch.
        backend.m_connected = true; backend.m_googleSync["fixture-account"] = QVariantMap{{"active", true}, {"owned", true}}; backend.m_account = "fixture-account";
        backend.m_enginePid = QCoreApplication::applicationPid(); backend.m_activeDraft = "draft-fixture"; backend.m_generation = 1;
        backend.m_draft = projection().toVariantMap();
        const QVariantList activeAccounts{QVariantMap{{"id", "fixture-account"}, {"name", "Fixture account"},
            {"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}},
            {"google", QVariantMap{{"capabilities", QVariantMap{{"send", true}, {"mail_delivery", true}}}}}},
            QVariantMap{{"id", "other-account"}, {"name", "Other active account"},
                {"lifecycle", QVariantMap{{"state", "active"}, {"revision", 1}}}}};
        backend.m_allAccounts = activeAccounts;
        backend.m_allAccounts.append(QVariantMap{{"id", "archived-account"}, {"name", "Archived account"}, {"read_only", true},
            {"lifecycle", QVariantMap{{"state", "removed"}, {"revision", 2}}}});
        require(backend.publishAccounts(activeAccounts), "valid active mailbox inventory");
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(fixture.server.serverName());
        await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached native socket connects");
        QString peerError;
        require(verifyEnginePeer(backend.m_socket.socketDescriptor(), QCoreApplication::applicationPid(), peerError), "cached native peer verified");
        bool hello = false;
        backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto result, bool ok) { hello = ok && result["mode"] == "live"; });
        await([&] { return hello; }, "cached native hello completes");
        QPointer<QLocalSocket> savePeer, readPeer;
        QString saveId, readId, operationMethod, readMethod;
        QJsonObject saveParams, operationParams, readParams;
        int saves = 0, reads = 0, heldAuxiliary = 0;
        fixture.customRequest = [&](QLocalSocket *peer, const QJsonObject &request) {
            const auto method = request["method"].toString(), id = request["id"].toString();
            if (method == "accounts") { peer->write(reply(id, {{"accounts", QJsonArray::fromVariantList(backend.m_allAccounts)}})); return true; }
            if (method == "view_save") { peer->write(reply(id, {{"revision", 1}})); return true; }
            // The draft chips list this account's drafts.
            if (method == "drafts") { peer->write(reply(id, {{"drafts", QJsonArray{}}, {"total", 0}})); return true; }
            // A new message looks up recent mail with its first recipient.
            if (method == "conversations") { peer->write(reply(id, {{"conversations", QJsonArray{}}, {"filtered_conversation_count", 0}})); return true; }
            if (method == "google_sent") {
                require(!request["account"].toString().isEmpty(), "outgoing read keeps its selected account");
                peer->write(reply(id, {{"account",request["account"]},{"outgoing",QJsonArray{}},{"next_offset",QJsonValue::Null}})); return true;
            }
            require(request["account"] == "fixture-account", "explicit Send/cached request account");
            if (method == "google_send_reconcile" && request["params"].toObject()["request_id"].toString().startsWith("held-cap-")) {
                ++heldAuxiliary; peer->setProperty("operation", true); return true;
            }
            if (method == "draft_save") { savePeer = peer; saveId = id; saveParams = request["params"].toObject(); ++saves; return true; }
            if (method == "draft_get" || method == "google_send_status") {
                readPeer = peer; readId = id; readMethod = method; readParams = request["params"].toObject(); ++reads; return true;
            }
            require(method == "google_send" || method == "google_send_reconcile" || method == "google_send_abandon", "only exact send operation routes");
            operationMethod = method; operationParams = request["params"].toObject();
            ++fixture.operations; fixture.operationPeer = peer; peer->setProperty("operation", true); return true;
        };
        QTemporaryDir copy; require(copy.isValid(), "private QML source copy");
        const auto modulePath = copy.filePath("SendTest"); require(QDir().mkpath(modulePath), "native send test module");
        for (const auto name : {"FmScrollBar.qml","Theme.qml", "FmDialog.qml", "FmConfirmDialog.qml", "FmButton.qml", "FmToolTip.qml", "FmIcon.qml", "RecipientField.qml", "ReplyComposer.qml"})
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + name, modulePath + '/' + name), "copy unchanged production QML");
        QFile module(modulePath + "/qmldir"); require(module.open(QIODevice::WriteOnly), "send test QML registration");
        module.write("module SendTest\nsingleton Theme 1.0 Theme.qml\nReplyComposer 1.0 ReplyComposer.qml\nFmButton 1.0 FmButton.qml\nFmToolTip 1.0 FmToolTip.qml\nFmIcon 1.0 FmIcon.qml\nRecipientField 1.0 RecipientField.qml\n"); module.close();
        qmlRegisterType<ReplyDocument>("SendTest", 1, 0, "ReplyDocument");
        QQmlApplicationEngine qml; FmIconItem::registerType(); bool qmlError = false;
        QObject::connect(&qml, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { qmlError = true; for (const auto &error : errors) qWarning() << error.toString(); });
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
        qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import SendTest 1.0
ApplicationWindow { width: 900; height: 1200; visible: true
    property string fixtureTextSize: "default"
    onFixtureTextSizeChanged: Theme.textSize = fixtureTextSize
    ReplyComposer { objectName: "composer"; width: parent.width }
})QML", QUrl::fromLocalFile(copy.filePath("Harness.qml")));
        require(!qml.rootObjects().isEmpty(), "actual ReplyComposer QML loads");
        auto root = qml.rootObjects().first();
        const auto recoveryGeometry = [&] {
            auto window = qobject_cast<QQuickWindow *>(root);
            auto composer = qobject_cast<QQuickItem *>(named(root, "composer"));
            require(window && composer, "native recovery geometry uses the actual composer");
            for (const auto size : {"default", "large"}) {
                // The supported 470px reader has two 24px margins; the composer
                // then reserves its own 20px left inset, leaving 402px for actions.
                window->setWidth(422); root->setProperty("fixtureTextSize", size);
                bool painted = false;
                const auto connection = QObject::connect(window, &QQuickWindow::frameSwapped, window, [&] { painted = true; });
                window->requestUpdate(); await([&] { return painted; }, "narrow recovery layout reaches a real frame");
                QObject::disconnect(connection);
                require(qAbs(composer->width() - 422) < .5, "composer matches the supported narrow reader");
                // The composer's fields start 18px in from its gold edge.
                const QRectF available(18, 0, 404, composer->height());
                QList<QRectF> previous;
                for (const auto name : {"checkGoogleSend", "resumeGoogleSend", "abandonGoogleSend"}) {
                    auto button = qobject_cast<QQuickItem *>(named(root, name));
                    require(button && button->isVisible() && button->isEnabled(), "all prepared recovery actions remain available");
                    const auto font = button->property("font").value<QFont>();
                    require(QFontInfo(font).family() == "Geist" && QFontInfo(font).pixelSize() == (QString(size) == "large" ? 14 : 13),
                        "recovery geometry uses the app font and saved text scale");
                    const auto bounds = button->mapRectToItem(composer, QRectF(0, 0, button->width(), button->height()));
                    qInfo() << "Prepared recovery geometry" << size << name << bounds << "available" << available;
                    require(available.adjusted(-.5, -.5, .5, .5).contains(bounds), "prepared recovery action stays inside the supported reader");
                    require(button->width() + .5 >= button->implicitWidth(), "prepared recovery label is fully visible");
                    for (const auto &other : previous) require(!bounds.intersects(other), "prepared recovery actions do not overlap");
                    previous.append(bounds);
                }
            }
            window->setWidth(900); root->setProperty("fixtureTextSize", "default");
        };
        const auto click = [&](const char *name) {
            auto control = named(root, name);
            require(control && control->property("enabled").toBool() && control->property("visible").toBool(), name);
            require(QMetaObject::invokeMethod(control, "clicked"), "actual native QML handler invokes");
        };
        const auto localEdit = [&](const QString &text) { backend.editFormattedDraft(text, "<p>" + text + "</p>"); backend.m_draftTimer.stop(); };
        QJsonObject googleOverride;
        const auto finishSave = [&](bool ok = true) {
            require(savePeer && !saveId.isEmpty(), "held actual save exists");
            auto saved = projection(saveParams["expected_revision"].toInteger() + 1);
            if (!googleOverride.isEmpty()) saved["google"] = googleOverride;
            for (const auto key : {"subject", "body", "body_html", "signature_text", "attachment_ids", "to_recipients", "cc_recipients", "bcc_recipients"})
                if (saveParams.contains(key)) saved[key] = saveParams[key];
            const auto id = std::exchange(saveId, {});
            savePeer->write(ok ? reply(id, saved) : refusal(id));
            await([&] { return !backend.m_savingDraft; }, "actual save receipt finishes");
        };
        const auto complete = [&](const QJsonObject &result) {
            fixture.complete(result); await([&] { return backend.m_googleOperations.isEmpty(); }, "actual auxiliary completion ends owner");
        };
        const auto finishRead = [&](const QJsonObject &result) {
            require(readPeer && !readId.isEmpty(), "held cached status exists");
            readPeer->write(reply(std::exchange(readId, {}), result));
            await([&] { return backend.m_inFlight.isEmpty(); }, "cached status callback finishes");
        };

        // Invalid unfinished routing must not arm Send for a later autosave.
        backend.stageDraftRecipient("to_recipients", "not an address"); click("captureReply");
        require(backend.m_googleSendPending.isEmpty() && saves == 0 && fixture.operations == 0, "invalid recipient input never arms Send");
        backend.stageDraftRecipient("to_recipients", ""); backend.clearError();
        localEdit("Version before save"); click("captureReply");
        await([&] { return saves == 1; }, "Send first uses ordinary durable save");
        localEdit("Changed while saving"); localEdit("Version before save");
        finishSave();
        require(fixture.operations == 0 && backend.m_googleSendPending.isEmpty(), "edit then revert before save receipt still cancels Send");
        require(named(root, "googleSendStatus")->property("text").toString().contains("press Send again"), "cancelled continuation is visible");

        localEdit("Failure must clear Send"); click("captureReply"); await([&] { return saves == 2; }, "second explicit save held");
        finishSave(false); backend.clearError();
        require(backend.m_googleSendPending.isEmpty() && fixture.operations == 0, "failed save clears Send continuation");
        backend.saveDraft(); await([&] { return saves == 3; }, "ordinary save retry starts");
        // Autosave and the background Gmail copy come and go while typing; Send stays enabled.
        require(backend.googleSend()["can_send"].toBool() && !backend.googleSend()["send_now"].toBool()
            && named(root, "captureReply")->property("enabled").toBool(), "an autosave on its way leaves Send enabled");
        finishSave();
        require(fixture.operations == 0, "later ordinary save cannot send implicitly");
        auto uploading = backend.m_draft["google"].toMap(); uploading["state"] = "in_flight"; backend.m_draft["google"] = uploading; emit backend.draftChanged();
        require(backend.googleSend()["can_send"].toBool() && !backend.googleSend()["send_now"].toBool()
            && named(root, "captureReply")->property("enabled").toBool(), "the background Gmail copy leaves Send enabled");
        uploading["state"] = "local"; backend.m_draft["google"] = uploading; emit backend.draftChanged();

        // Commit staged Bcc before the saved-version snapshot is armed.
        backend.stageDraftRecipient("bcc_recipients", "Private <private@example.test>");
        click("captureReply"); await([&] { return saves == 4; }, "recipient commit is saved first");
        require(saveParams["bcc_recipients"].toArray().size() == 1, "staged Bcc is part of saved routing");
        // Gmail is still taking the background copy when the save lands: Send waits for it, then sends.
        googleOverride = QJsonObject::fromVariantMap(uploading); googleOverride["state"] = "in_flight";
        googleOverride["local_revision"] = saveParams["expected_revision"].toInteger() + 1;
        finishSave(); googleOverride = {};
        require(fixture.operations == 0 && backend.m_googleSendPending.contains("waiting_since") && backend.m_googleSendWaitTimer.isActive()
            && backend.googleSend()["busy"].toBool(), "Send pressed during the Gmail upload waits for it");
        auto settled = backend.m_draft["google"].toMap(); settled["state"] = "local"; backend.m_draft["google"] = settled; emit backend.draftChanged();
        await([&] { return fixture.operations == 1; }, "one explicit Send reaches authenticated auxiliary IPC");
        const auto first = operationParams;
        require(operationMethod == "google_send" && first.size() == 4 && first["draft_id"] == "draft-fixture"
            && first["expected_revision"].toInteger() == backend.m_draft["revision"].toLongLong()
            && first["expected_google_revision"] == 0 && !first["request_id"].toString().isEmpty(), "exact frozen send request, with new twin revision zero");
        require(!backend.draftLocked() && !backend.googleDraft()["can_replace"].toBool(), "send leaves editor open while blocking B1 replacement");
        localEdit("Newer body while Google owns frozen version"); const auto later = authored(backend.m_draft);
        auto accepted = status(first, "accepted", 3, 8, first["expected_revision"].toInteger());
        accepted["body"] = "must never replace editor"; complete(accepted);
        require(authored(backend.m_draft) == later && backend.m_draftDirty, "accepted earlier version cannot clear newer editor content");
        require(qobject_cast<ReplyDocument *>(named(root, "replyDocument"))->plainText() == later["body"].toString(), "actual QML editor keeps newer text");
        require(backend.googleSend()["can_send"].toBool() && !backend.googleDraft()["can_check"].toBool()
            && named(root, "googleSendStatus")->property("text").toString().contains("creates another email"), "newer explicit email is disclosed and consumed B1 target stays blocked");
        backend.m_draftDirty = false; emit backend.draftChanged();
        require(!backend.googleSend()["can_send"].toBool(), "same accepted saved revision cannot send again");

        // A prepared resume uses the exact earlier request even when the editor
        // has changed. Abandon is bound to the selected operation revision.
        auto second = first; second["request_id"] = "second-frozen-request"; second["expected_revision"] = first["expected_revision"].toInteger() + 1; second["expected_google_revision"] = 8;
        auto prepared = status(second, "prepared", 1, 10, first["expected_revision"].toInteger());
        backend.m_googleSendUi.clear(); backend.mergeGoogleSendStatus(prepared, "fixture-account", "draft-fixture"); emit backend.draftChanged();
        recoveryGeometry();
        localEdit("Later edits cannot retarget resume"); click("resumeGoogleSend");
        await([&] { return fixture.operations == 2; }, "prepared resume reaches auxiliary IPC");
        require(operationParams == second, "resume reuses all frozen parameters verbatim");
        const auto resumeEdits = authored(backend.m_draft); complete(status(second, "prepared", 2, 11, first["expected_revision"].toInteger()));
        require(authored(backend.m_draft) == resumeEdits, "prepared outcome preserves current edits");
        click("abandonGoogleSend"); await([&] { return fixture.operations == 3; }, "explicit abandon reaches IPC");
        require(operationMethod == "google_send_abandon" && operationParams == QJsonObject{{"request_id", second["request_id"]}, {"expected_revision", 2}}, "abandon uses operation revision rather than draft revision");
        complete(status(second, "abandoned", 3, 12, first["expected_revision"].toInteger()));

        auto unknown = status(second, "acceptance_unknown", 4, 13, first["expected_revision"].toInteger());
        backend.m_googleSendUi.clear(); backend.mergeGoogleSendStatus(unknown, "fixture-account", "draft-fixture"); emit backend.draftChanged();
        require(!backend.googleSend()["can_send"].toBool() && !backend.googleSend()["can_resume"].toBool(), "unknown prevents every send and resume");
        click("checkGoogleSend"); await([&] { return fixture.operations == 4; }, "unknown explicit Check reaches read-only route");
        require(operationMethod == "google_send_reconcile" && operationParams == QJsonObject{{"request_id", second["request_id"]}}, "unknown Check contains exact prior request only");
        unknown["revision"] = 5; unknown["current_google_revision"] = 14;
        unknown["read_evidence"] = QJsonObject{{"matches", QJsonArray{QJsonObject{{"message_id", "candidate"}}}}, {"truncated", false}};
        complete(unknown);
        require(named(root, "googleSendStatus")->property("text").toString().contains("original outcome remains unknown")
            && !backend.googleSend()["can_resume"].toBool() && fixture.operations == 4, "matching Sent evidence does not imply acceptance or retry");

        click("checkGoogleSend"); await([&] { return fixture.operations == 5; }, "foreign response scenario begins");
        auto foreign = unknown; foreign["draft_id"] = "other-draft"; complete(foreign);
        require(backend.m_draft["google_send"].toMap()["revision"].toLongLong() == 5 && !backend.googleSend()["can_resume"].toBool(), "foreign receipt cannot advance metadata");
        click("checkGoogleSend"); await([&] { return reads == 1; }, "unconfirmed response checks cached exact status first");
        require(readMethod == "google_send_status" && readParams == QJsonObject{{"request_id", second["request_id"]}}, "lost-result recovery never creates or submits a request");
        finishRead(unknown);

        // Historical selected receipt is displayed separately, never merged as
        // current even with a larger current-twin watermark.
        click("checkGoogleSend"); await([&] { return fixture.operations == 6; }, "held historical reply begins");
        auto third = second; third["request_id"] = "newer-request";
        auto newerStatus = status(third, "prepared", 1, 20);
        backend.mergeGoogleSendStatus(newerStatus, "fixture-account", "draft-fixture"); emit backend.draftChanged();
        auto historical = unknown; historical["operation_is_current"] = false; historical["current_google_revision"] = 21;
        complete(historical); await([&] { return reads == 2; }, "historical receipt requests current cached metadata");
        auto currentProjection = projection(30); currentProjection["google_send"] = newerStatus;
        const auto beforeRead = authored(backend.m_draft); finishRead(currentProjection);
        require(authored(backend.m_draft) == beforeRead && backend.m_draft["google_send"].toMap()["request_id"] == "newer-request",
            "historical refresh merges current send metadata only");
        require(named(root, "googleSendStatus")->property("text").toString().contains("earlier request") && !backend.googleSend()["can_resume"].toBool(), "historical selected operation cannot operate on newer request");
        auto staleProjection = projection(31); staleProjection["google_send"] = status(second, "accepted", 50, 19);
        require(backend.mergedDraftProjection(staleProjection)["google_send"].toMap()["request_id"] == "newer-request", "different-operation cached metadata uses twin watermark rather than per-operation revision");
        click("showCurrentGoogleSend");
        require(backend.googleSend()["can_resume"].toBool() && backend.googleSend()["request_id"] == "newer-request" && fixture.operations == 6,
            "viewing current request restores its exact prepared controls without invoking Send");

        // The auxiliary owner may complete after the user changes draft/account;
        // only its retained exact receipt changes, never the visible editor.
        backend.m_googleSendUi.clear(); backend.m_draft["google_send"] = unknown.toVariantMap(); emit backend.draftChanged();
        click("checkGoogleSend"); await([&] { return fixture.operations == 7; }, "held operation before switching begins");
        backend.m_account = "other-account"; backend.m_activeDraft = "other-draft";
        auto other = projection(1); other["account"] = "other-account"; other["draft_id"] = "other-draft";
        backend.m_draft = other.toVariantMap(); ++backend.m_generation; emit backend.draftChanged();
        const auto switched = backend.m_draft; complete(unknown);
        require(backend.m_draft == switched, "late exact receipt never changes switched account/draft");
        backend.m_account = "fixture-account"; backend.m_activeDraft = "draft-fixture";
        auto oldProjection = projection(5);
        require(backend.mergedDraftProjection(oldProjection)["google_send"].toMap()["state"] == "acceptance_unknown", "returning to draft retains known unknown outcome over older cache");

        backend.m_googleSendUi.clear(); backend.m_draft = projection().toVariantMap(); backend.m_draftDirty = false; emit backend.draftChanged();
        click("captureReply"); await([&] { return fixture.operations == 8; }, "new original request reaches IPC before lost response");
        const auto lostRequest = operationParams; fixture.operationPeer->abort();
        await([&] { return backend.m_googleOperations.isEmpty(); }, "lost original response closes owner");
        require(!backend.googleSend()["can_send"].toBool() && !backend.googleSend()["can_resume"].toBool(), "lost original waits for exact cached status");
        click("checkGoogleSend"); await([&] { return reads == 3; }, "lost original status is queried");
        const auto errorFrame = [](const QString &id, const QString &code, const QJsonObject &details = QJsonObject{}) {
            return QJsonDocument(QJsonObject{{"version", 1}, {"id", id}, {"ok", false},
                {"error", QJsonObject{{"code", code}, {"message", "Fixture refusal"}, {"details", details}}}}).toJson(QJsonDocument::Compact) + '\n';
        };
        readPeer->write(errorFrame(std::exchange(readId, {}), "not_found"));
        await([&] { return backend.m_inFlight.isEmpty(); }, "not_found exact status completes"); backend.clearError();
        require(!backend.googleSend()["can_send"].toBool() && backend.googleSend()["can_resume"].toBool(), "not_found is not unsent proof; only original identity can be explicitly resumed");
        click("resumeGoogleSend"); await([&] { return fixture.operations == 9; }, "explicit original identity resume dispatches");
        require(operationParams == lostRequest, "not_found recovery preserves all four original fields");
        fixture.operationPeer->write(errorFrame("operation", "stale_revision", {{"send_invocation_finished_before_dispatch", true}}));
        await([&] { return backend.m_googleOperations.isEmpty(); }, "resumed invocation refusal finishes");
        require(!backend.googleSend()["can_send"].toBool() && backend.m_googleSendUi[backend.googleSendKey()].request == lostRequest,
            "predispatch proof from a later invocation cannot release original uncertainty");
        click("checkGoogleSend"); await([&] { return reads == 4; }, "original uncertainty checks exact status again");
        finishRead(status(lostRequest, "acceptance_unknown", 3, 40));
        require(!backend.googleSend()["can_resume"].toBool() && !backend.googleSend()["can_send"].toBool(), "recorded unknown removes original-resume action");

        backend.m_googleSendUi.clear(); backend.m_draft = projection().toVariantMap(); emit backend.draftChanged();
        click("captureReply"); await([&] { return fixture.operations == 10; }, "new attempt for settled predispatch proof begins");
        fixture.operationPeer->write(errorFrame("operation", "invalid_draft", {{"send_invocation_finished_before_dispatch", true}}));
        await([&] { return backend.m_googleOperations.isEmpty(); }, "fresh direct predispatch refusal completes");
        require(backend.googleSend()["can_send"].toBool() && backend.m_googleSendUi[backend.googleSendKey()].request.isEmpty(),
            "settled direct refusal releases only a fresh unambiguous attempt");

        localEdit("Navigation before save finishes"); click("captureReply"); await([&] { return saves == 5; }, "navigation test holds real save");
        const auto pendingBeforeRefusedNavigation = backend.m_googleSendPending;
        const auto draftBeforeRefusedNavigation = backend.m_draft;
        const auto generationBeforeRefusedNavigation = backend.m_generation;
        require(!pendingBeforeRefusedNavigation.isEmpty() && !backend.m_afterDraftSave, "held Send is armed before navigation checks");
        for (const auto target : {"unknown-account", "archived-account"}) {
            backend.selectAccount(target);
            require(backend.account() == "fixture-account" && backend.m_generation == generationBeforeRefusedNavigation
                && backend.m_draft == draftBeforeRefusedNavigation && backend.m_googleSendPending == pendingBeforeRefusedNavigation
                && !backend.m_afterDraftSave && saves == 5 && fixture.operations == 10,
                "unknown or archived mailbox selection cannot navigate, retarget or cancel the armed Send");
        }
        backend.selectAccount("other-account");
        require(backend.m_googleSendPending.isEmpty() && bool(backend.m_afterDraftSave), "actual mailbox selection cancels Send before deferred navigation");
        // Other tests own the mailbox hydration. Keep this fixture focused on
        // the actual pre-save navigation interception and its durable receipt.
        backend.m_afterDraftSave = {}; finishSave();
        require(fixture.operations == 10, "save after navigation intent cannot submit the cancelled version");
        backend.m_draft = projection(10).toVariantMap(); localEdit("Dirty editor during held cached Send status");
        auto cachedUnknown = status(lostRequest, "acceptance_unknown", 3, 50);
        backend.mergeGoogleSendStatus(cachedUnknown, "fixture-account", "draft-fixture");
        backend.loadActiveDraft(); await([&] { return reads == 5; }, "ordinary idle draft_get holds Send metadata");
        const auto dirtyBeforeCache = authored(backend.m_draft);
        backend.mergeGoogleSendStatus(status(lostRequest, "accepted", 4, 51, 5), "fixture-account", "draft-fixture");
        auto oldCached = projection(11); oldCached["google_send"] = cachedUnknown; oldCached["body"] = "stale cached author text";
        finishRead(oldCached);
        require(authored(backend.m_draft) == dirtyBeforeCache && backend.m_draft["google_send"].toMap()["state"] == "accepted",
            "actual older cached Send metadata cannot roll back accepted state or dirty authored text");

        // Fill the real shared auxiliary cap with authenticated, held owners.
        // A new Send refused here never creates a request and must remain usable.
        backend.m_googleSendUi.clear(); backend.m_draft = projection(20).toVariantMap(); backend.m_draftDirty = false; emit backend.draftChanged();
        for (int i = 0; i < 4; ++i) backend.googleOperation("google_send_reconcile", "fixture-account", {{"request_id", "held-cap-" + QString::number(i)}}, [](auto, bool) {});
        await([&] { return heldAuxiliary == 4; }, "four actual authenticated auxiliary owners are held");
        localEdit("Edited draft survives a local busy refusal"); click("captureReply");
        await([&] { return saves == 6; }, "busy-cap Send saves authored version first"); finishSave();
        require(backend.m_draft["body"] == "Edited draft survives a local busy refusal" && !backend.m_draftDirty
            && backend.m_draft["revision"].toLongLong() == 21 && fixture.operations == 10
            && backend.m_googleSendUi[backend.googleSendKey()].request.isEmpty() && backend.googleSend()["can_send"].toBool(),
            "never-created busy Send preserves saved edit and clears only its fresh request identity");
        backend.cancelGoogleOperation("fixture-account", "held-cap-0");
        require(backend.m_googleOperations.size() == 3, "one auxiliary seat is released");
        click("captureReply"); await([&] { return fixture.operations == 11; }, "later explicit Send works after a seat is available");
        const auto capRequest = operationParams;
        require(capRequest["expected_revision"] == 21 && operationMethod == "google_send", "later explicit Send uses retained saved version");
        fixture.operationPeer->abort(); await([&] { return backend.m_googleOperations.size() == 3; }, "later Send transport uncertainty is retained");
        click("checkGoogleSend"); await([&] { return reads == 6; }, "uncertain Send checks its exact cached identity");
        readPeer->write(errorFrame(std::exchange(readId, {}), "not_found", {{"send_request_obsolete_without_dispatch", true}}));
        await([&] { return backend.m_inFlight.isEmpty(); }, "uncertain cached not_found completes"); backend.clearError();
        require(backend.m_googleSendUi[backend.googleSendKey()].uncertain && !backend.googleSend()["can_send"].toBool(),
            "stronger proof on a different RPC cannot release the original Send identity");
        backend.googleOperation("google_send_reconcile", "fixture-account", {{"request_id", "held-cap-4"}}, [](auto, bool) {});
        await([&] { return heldAuxiliary == 5; }, "auxiliary cap is filled again");
        click("resumeGoogleSend");
        require(fixture.operations == 11 && backend.m_googleSendUi[backend.googleSendKey()].request == capRequest
            && backend.m_googleSendUi[backend.googleSendKey()].uncertain && !backend.googleSend()["can_send"].toBool(),
            "local busy proof cannot erase uncertainty from the previous invocation");
        backend.cancelGoogleOperations();
        localEdit("Newer saved work after the original reply was lost"); backend.saveDraft();
        await([&] { return saves == 7; }, "newer authored work saves while original Send remains uncertain"); finishSave();
        require(backend.m_draft["revision"].toLongLong() == 22 && backend.m_googleSendUi[backend.googleSendKey()].request == capRequest,
            "saving newer work cannot itself release the original request");
        click("resumeGoogleSend"); await([&] { return fixture.operations == 12; }, "explicit obsolete original Resume reaches engine");
        require(operationParams == capRequest, "obsolete Resume still carries exact original four fields");
        fixture.operationPeer->write(errorFrame("operation", "stale_revision"));
        await([&] { return backend.m_googleOperations.isEmpty(); }, "generic stale error returns");
        require(backend.m_googleSendUi[backend.googleSendKey()].uncertain && !backend.googleSend()["can_send"].toBool(),
            "generic stale revision is not an all-invocation proof");
        click("resumeGoogleSend"); await([&] { return fixture.operations == 13; }, "same original request is explicitly resumed again");
        require(operationParams == capRequest, "strong proof is correlated with exact original request");
        localEdit("Unsaved text typed while obsolete proof is pending"); const auto beforeProof = authored(backend.m_draft);
        fixture.operationPeer->write(errorFrame("operation", "stale_revision", {{"send_request_obsolete_without_dispatch", true}}));
        await([&] { return backend.m_googleOperations.isEmpty(); }, "exact all-invocation obsolete proof returns");
        require(authored(backend.m_draft) == beforeProof && backend.m_draftDirty && fixture.operations == 13
            && backend.m_googleSendUi[backend.googleSendKey()].request.isEmpty() && backend.googleSend()["can_send"].toBool(),
            "strong obsolete proof clears only original identity, preserves newer text and requires fresh explicit Send");
        backend.m_draftDirty = false; backend.m_googleSendUi.clear(); backend.m_draft = projection().toVariantMap();
        backend.m_googleSync.clear(); emit backend.googleSyncChanged();
        require(!backend.googleSend()["can_send"].toBool() && backend.googleSend()["can_resume_sync"].toBool()
            && backend.googleSend()["status"].toString().contains("Resume sync"), "paused sync offers a plain recovery action before Send");
        const auto currentRequest = QJsonObject{{"draft_id","draft-fixture"},{"request_id","final-sent"},{"expected_revision",5},{"expected_google_revision",0}};
        backend.mergeGoogleSendStatus(status(currentRequest,"accepted",3,2,5),"fixture-account","draft-fixture");emit backend.draftChanged();
        require(backend.googleSend()["sent_current"].toBool() && backend.googleSend()["status"]=="Sent."
            && !backend.googleSend()["can_check"].toBool() && !backend.googleDraft()["visible"].toBool(),"accepted unchanged draft has only plain sent feedback");
        backend.m_draftDirty=true;
        require(!backend.googleSend()["sent_current"].toBool(),"unsaved newer work never closes as sent");backend.m_draftDirty=false;
        // Startup inventory and preferences arrive independently; only established eligible accounts resume once.
        backend.m_draftTimer.stop();backend.m_pollTimer.stop();backend.m_viewTimer.stop();
        auto startupAccount=[](const QString &id,const QString &state="active") {
            return QVariantMap{{"id",id},{"name",id},{"lifecycle",QVariantMap{{"state",state},{"revision",1}}},
                {"google",QVariantMap{{"connection_state","connected"},{"sync",QVariantMap{{"state","caught_up"}}}}}};
        };
        backend.m_allAccounts={startupAccount("fixture-account"),startupAccount("paused"),startupAccount("removed","removed")};
        backend.publishAccounts({backend.m_allAccounts[0],backend.m_allAccounts[1]});
        backend.m_initialAccountsLoaded=true;backend.m_initialPreferencesLoaded=false;
        backend.m_preferences["google_sync_services"]=QVariantMap{{"paused",QVariantMap{{"mail",false},{"calendar",false}}}};
        backend.m_startupGoogleAccounts={"fixture-account","paused","removed"};int syncStarts=0;
        fixture.customRequest=[&](QLocalSocket *peer,const QJsonObject &request) {
            const auto method=request["method"].toString(),id=request["id"].toString();
            if(method=="google_sync_start") {require(request["account"]=="fixture-account","only eligible startup account resumes");++syncStarts;peer->write(reply(id,{{"started",true}}));}
            else if(method=="google_sync_status") peer->write(reply(id,{{"accounts",QJsonArray{}},{"limits",QJsonObject{}}}));
            else if(method=="accounts") peer->write(reply(id,{{"accounts",QJsonArray::fromVariantList(backend.m_allAccounts)}}));
            else if(method=="view_save") peer->write(reply(id,{{"revision",1}}));
            // The account refresh may also reload Sent's outgoing status.
            else if(method=="google_sent") peer->write(reply(id,{{"account",request["account"]},{"outgoing",QJsonArray{}},{"next_offset",QJsonValue::Null}}));
            else return false;
            return true;
        };
        backend.resumeStartupGoogleSync();require(backend.m_startupGoogleAccounts.size()==3,"startup waits for saved service preferences");
        backend.m_initialPreferencesLoaded=true;backend.resumeStartupGoogleSync();
        await([&]{return syncStarts==1 && backend.m_inFlight.isEmpty();},"startup sync and inventory refresh finish");
        backend.resumeStartupGoogleSync();require(syncStarts==1 && backend.m_startupGoogleAccounts.isEmpty(),"polling cannot restart a paused or denied startup attempt");
        require(!qmlError, "native Send controls generated no QML errors");
        backend.m_draftTimer.stop(); backend.m_pollTimer.stop(); backend.m_viewTimer.stop();
        qInfo() << "Native Send controls passed: actual QML handlers, private held save/auxiliary IPC, recipient validation, cancelled save continuation, frozen resume, revision-bound abandon, unknown evidence, historical/current metadata and account/editor isolation. Synthetic IPC only; no provider or performance claim.";
    }
};
int main(int argc, char **argv) {
    QQuickStyle::setStyle("Basic"); QGuiApplication app(argc, argv);
    GoogleSendControlsTest::run(); return 0;
}
