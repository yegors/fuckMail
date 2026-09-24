#include "GoogleOperationTestFixture.h"
#include "FmIconItem.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QJsonArray>
#include <QJSValue>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>

using namespace GoogleOperationFixture;

namespace {
QVariant unwrapped(QVariant value) {
    return value.metaType() == QMetaType::fromType<QJSValue>() ? value.value<QJSValue>().toVariant() : value;
}
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
QVariantMap authored(QVariantMap draft) { draft.remove("google"); return draft; }
QJsonObject status(const QString &state, qint64 revision, qint64 localRevision = 5) {
    return {{"account", "fixture-account"}, {"draft_id", "draft-fixture"}, {"state", state}, {"revision", revision},
        {"local_revision", localRevision}, {"operation_is_current", true}, {"operation", QJsonValue::Null},
        {"baseline", QJsonObject{{"draft_id", "provider-draft"}, {"message_id", "provider-message"},
            {"observed_sha256", QString(64, 'a')}, {"upload_sha256", QString(64, 'a')}, {"draft_revision", localRevision}}},
        {"conflict", state == "conflict" ? QJsonValue(QJsonObject{{"reason", "remote_changed"}, {"observed", QJsonValue::Null}}) : QJsonValue::Null}};
}
QJsonObject projection(qint64 revision, const QJsonObject &google) {
    return {{"account", "fixture-account"}, {"draft_id", "draft-fixture"}, {"conversation_id", QJsonValue::Null},
        {"mode", "new"}, {"revision", revision}, {"discarded", false}, {"from", "me@example.test"}, {"from_name", "Fixture author"},
        {"subject", "Human subject 日本語"}, {"body", "Exact authored body café"}, {"body_html", QJsonValue::Null},
        {"signature_text", "Signature"}, {"attachment_ids", QJsonArray{}}, {"attachments", QJsonArray{}},
        {"to_recipients", QJsonArray{}}, {"cc_recipients", QJsonArray{}}, {"bcc_recipients", QJsonArray{}}, {"google", google}};
}
}

class GoogleDraftControlsTest {
public:
    static void run() {
        Engine fixture;
        MailBackend backend(fixture.profile, "unused-fixture-engine", nullptr, false, true);
        backend.m_connected = true; backend.m_account = "fixture-account";
        backend.m_enginePid = QCoreApplication::applicationPid();
        backend.m_activeDraft = "draft-fixture"; backend.m_generation = 1;
        backend.m_draft = projection(5, status("synced", 7)).toVariantMap();
        backend.m_accounts = {QVariantMap{{"id", "fixture-account"}, {"name", "Fixture account"},
            {"google", QVariantMap{{"capabilities", QVariantMap{{"mail_delivery", true}}}}}}};
        // Use the actual cached request/response queue without starting unrelated
        // account/bootstrap workers. The private peer and hello still verify.
        QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
        backend.m_socket.connectToServer(fixture.server.serverName());
        await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached native socket connects");
        QString peerError;
        require(verifyEnginePeer(backend.m_socket.socketDescriptor(), QCoreApplication::applicationPid(), peerError), "cached native peer verified");
        bool hello = false;
        backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto result, bool ok) { hello = ok && result["mode"] == "live"; });
        await([&] { return hello; }, "cached native hello completes");
        QPointer<QLocalSocket> cachedPeer; QString cachedId; int reads = 0;
        fixture.customRequest = [&](QLocalSocket *peer, const QJsonObject &request) {
            if (request["method"] != "draft_get") return false;
            require(request["account"] == "fixture-account" && request["params"].toObject() == QJsonObject{{"draft_id", "draft-fixture"}}, "exact cached draft request");
            cachedPeer = peer; cachedId = request["id"].toString(); ++reads; return true;
        };

        QTemporaryDir copy; require(copy.isValid(), "private QML source copy");
        const auto modulePath = copy.filePath("DraftTest"); require(QDir().mkpath(modulePath), "native draft test module");
        for (const auto name : {"FmScrollBar.qml","Theme.qml", "FmDialog.qml", "FmConfirmDialog.qml", "FmButton.qml", "FmToolTip.qml", "FmIcon.qml", "RecipientField.qml", "ReplyComposer.qml"})
            require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + name, modulePath + '/' + name), "copy unchanged production QML");
        QFile module(modulePath + "/qmldir"); require(module.open(QIODevice::WriteOnly), "draft test QML registration");
        module.write("module DraftTest\nsingleton Theme 1.0 Theme.qml\nReplyComposer 1.0 ReplyComposer.qml\nFmButton 1.0 FmButton.qml\nFmToolTip 1.0 FmToolTip.qml\nFmIcon 1.0 FmIcon.qml\nRecipientField 1.0 RecipientField.qml\n"); module.close();
        qmlRegisterType<ReplyDocument>("DraftTest", 1, 0, "ReplyDocument");
        QQmlApplicationEngine qml; FmIconItem::registerType();
        bool qmlError = false;
        QObject::connect(&qml, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { qmlError = true; for (const auto &error : errors) qWarning() << error.toString(); });
        qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
        qml.loadData(R"QML(import QtQuick
import QtQuick.Controls
import DraftTest 1.0
ApplicationWindow { width: 900; height: 1000; visible: true
    ReplyComposer { objectName: "composer"; width: parent.width }
})QML", QUrl::fromLocalFile(copy.filePath("Harness.qml")));
        require(!qml.rootObjects().isEmpty(), "actual ReplyComposer QML loads");
        auto root = qml.rootObjects().first();
        const auto click = [&](const char *name) {
            auto control = named(root, name);
            require(control && control->property("enabled").toBool() && control->property("visible").toBool(), name);
            require(QMetaObject::invokeMethod(control, "clicked"), "actual native QML handler invokes");
        };
        const auto localEdit = [&](const QString &text) {
            backend.editFormattedDraft(text, "<p>" + text + "</p>"); backend.m_draftTimer.stop();
        };
        const auto setStatus = [&](const QJsonObject &google) {
            require(backend.mergeGoogleDraftStatus(google, "fixture-account", "draft-fixture"), "publish newer actual metadata helper");
            emit backend.draftChanged();
        };
        const auto startRead = [&] {
            const auto expected = reads + 1; backend.loadActiveDraft();
            await([&] { return reads == expected; }, "ordinary cached draft_get reaches independent connection");
        };
        const auto finishRead = [&](const QJsonObject &result) {
            require(cachedPeer && !cachedId.isEmpty(), "held cached response exists");
            cachedPeer->write(reply(cachedId, result));
            await([&] { return backend.m_pending.isEmpty(); }, "actual cached callback delivered");
        };
        const auto prepareCheck = [&] {
            fixture.expectedMethod = "google_draft_reconcile"; fixture.expectedParams = {{"draft_id", "draft-fixture"}};
        };
        const auto waitOperation = [&](int expected) { await([&] { return fixture.operations == expected; }, "recovery uses authenticated auxiliary connection"); };
        const auto complete = [&](const QJsonObject &result) {
            fixture.complete(result); await([&] { return backend.m_googleDraftOperationId.isEmpty(); }, "native recovery callback finishes");
        };
        const auto openReplacement = [&] {
            click("replaceGoogleDraft");
            auto dialog = named(root, "googleDraftReplaceDialog");
            await([&] { return dialog && dialog->property("opened").toBool(); }, "actual replacement confirmation opens");
            return QJsonObject::fromVariantMap(unwrapped(dialog->property("capturedContext")).toMap());
        };
        const auto expectReplacement = [&](QJsonObject captured) {
            require(captured["account"] == "fixture-account" && captured["draft_id"] == "draft-fixture"
                && !captured["request_id"].toString().isEmpty() && captured["action"] == "replace_local", "modal captures exact target before confirmation");
            captured.remove("account"); captured.remove("generation");
            fixture.expectedMethod = "google_draft_resolve"; fixture.expectedParams = captured;
        };
        require(!named(root, "captureReply")->property("enabled").toBool(), "real Send remains disabled");

        // Cache and auxiliary completion order is deliberately inverted. A newer
        // authored revision in the older response cannot lower Google metadata.
        startRead(); setStatus(status("conflict", 9));
        finishRead(projection(6, status("synced", 8, 6)));
        require(backend.m_draft["revision"].toLongLong() == 6 && backend.m_draft["google"].toMap()["revision"].toLongLong() == 9,
            "newer local projection retains the newer Google revision");
        localEdit("Newer unsaved Unicode 日本語"); const auto edited = authored(backend.m_draft);
        startRead(); finishRead(projection(6, status("acknowledged", 10, 6)));
        require(authored(backend.m_draft) == edited && backend.m_draftDirty && backend.m_draft["google"].toMap()["revision"].toLongLong() == 10,
            "idle cached metadata advances without replacing unsaved fields");
        require(qobject_cast<ReplyDocument *>(named(root, "replyDocument"))->plainText() == edited["body"].toString(), "actual editor retains authored text through metadata signal");
        startRead(); auto foreign = projection(20, status("synced", 30, 20)); foreign["account"] = "another-account";
        finishRead(foreign); require(authored(backend.m_draft) == edited && backend.m_draft["google"].toMap()["revision"].toLongLong() == 10, "foreign cached projection is refused");

        setStatus(status("acceptance_unknown", 11, 6)); prepareCheck();
        require(!backend.googleDraft()["can_replace"].toBool(), "unknown upload offers only readback");
        click("checkGoogleDraft"); waitOperation(1);
        require(!backend.m_googleOperations.isEmpty() && backend.m_socket.state() == QLocalSocket::ConnectedState, "auxiliary work preserves the cached connection");
        auto wrong = status("synced", 12, 6); wrong["draft_id"] = "another-draft";
        complete(wrong); require(authored(backend.m_draft) == edited && backend.m_draft["google"].toMap()["revision"].toLongLong() == 11, "mismatched auxiliary draft cannot apply status or author fields");
        click("checkGoogleDraft"); waitOperation(2);
        auto other = projection(1, status("local", 0, 1)); other["account"] = "other-account"; other["draft_id"] = "other-draft";
        backend.m_account = "other-account"; backend.m_activeDraft = "other-draft"; backend.m_draft = other.toVariantMap(); ++backend.m_generation; emit backend.draftChanged();
        const auto switched = backend.m_draft; complete(status("synced", 15, 6));
        require(backend.m_draft == switched, "late auxiliary response leaves another account and draft unchanged");
        backend.m_account = "fixture-account"; backend.m_activeDraft = "draft-fixture";
        backend.m_draft = projection(6, status("conflict", 20, 6)).toVariantMap(); backend.m_draftDirty = false; ++backend.m_generation; emit backend.draftChanged();

        const auto initialOperations = fixture.operations;
        openReplacement(); localEdit("Edited while confirmation was open"); click("confirmReplaceGoogleDraft");
        require(fixture.operations == initialOperations && backend.m_draftDirty && backend.m_draft["body"] == "Edited while confirmation was open", "confirmation refuses dirty context without autosave or dispatch");
        backend.m_draftDirty = false; emit backend.draftChanged();
        openReplacement(); setStatus(status("conflict", 21, 6)); click("confirmReplaceGoogleDraft");
        require(fixture.operations == initialOperations, "changed Google revision invalidates confirmation");
        openReplacement(); const auto importKey = backend.attachmentImportKey(); backend.m_attachmentImports.insert(importKey, {{"request_id", "held-import"}});
        click("confirmReplaceGoogleDraft"); require(fixture.operations == initialOperations, "pending import invalidates confirmation");
        backend.m_attachmentImports.remove(importKey); emit backend.draftChanged();
        auto captured = openReplacement(); expectReplacement(captured); const auto frozenAuthor = authored(backend.m_draft);
        click("confirmReplaceGoogleDraft"); waitOperation(3);
        require(fixture.operationParams["expected_revision"] == 6 && fixture.operationParams["expected_google_revision"] == 21, "dispatch uses the two captured revisions");
        localEdit("Typed during upload"); const auto duringUpload = authored(backend.m_draft);
        auto acknowledged = status("acknowledged", 22, 6); acknowledged["body"] = "provider response cannot author text";
        complete(acknowledged);
        require(authored(backend.m_draft) == duringUpload && backend.m_draftDirty && authored(backend.m_draft) != frozenAuthor, "late acknowledged replacement merges only metadata");
        require(fixture.operations == 3, "replacement issues one attempt");

        backend.m_draftDirty = false; setStatus(status("conflict", 23, 6)); captured = openReplacement(); expectReplacement(captured);
        click("confirmReplaceGoogleDraft"); waitOperation(4); fixture.operationPeer->abort();
        await([&] { return backend.m_googleDraftOperationId.isEmpty(); }, "lost auxiliary response is handled");
        require(!backend.googleDraft()["can_replace"].toBool() && backend.googleDraft()["can_check"].toBool() && fixture.operations == 4,
            "unconfirmed replacement allows Check Google without automatic replay");
        prepareCheck(); click("checkGoogleDraft"); waitOperation(5); click("cancelGoogleDraft");
        await([&] { return backend.m_googleDraftOperationId.isEmpty(); }, "actual Cancel closes the recovery owner");
        require(!backend.googleDraft()["can_replace"].toBool(), "cancelled readback does not clear uncertain replacement");
        click("checkGoogleDraft"); waitOperation(6); complete(status("conflict", 24, 6));
        require(backend.googleDraft()["can_replace"].toBool(), "fresh read-only result permits a new explicit confirmation");
        setStatus(status("synced", 25, 6));
        localEdit("Unsaved after Google checked the saved version");
        auto statusText = named(root, "googleDraftStatus")->property("text").toString();
        require(statusText.contains("Unsaved edits") && statusText.contains("local save") && statusText.contains("Google draft sync")
            && !statusText.contains("Google draft checked"), "actual editor change immediately shows unsaved sync work");
        auto differing = status("synced", 26, 6); auto baseline = differing["baseline"].toObject(); baseline["observed_sha256"] = QString(64, 'b'); differing["baseline"] = baseline;
        setStatus(differing);
        statusText = named(root, "googleDraftStatus")->property("text").toString();
        require(backend.googleDraft()["byte_mismatch"].toBool() && !backend.googleDraft()["warning"].toBool() && statusText.contains("Unsaved edits")
            && !named(root, "captureReply")->property("enabled").toBool(), "transport byte changes do not invent a content warning; unsaved Send stays disabled");
        differing["fidelity_warning"] = true;
        setStatus(differing);
        require(backend.googleDraft()["warning"].toBool() && backend.googleDraft()["status"].toString().contains("differs from your local version"), "proven content changes retain an actionable warning");
        auto saved = QJsonObject::fromVariantMap(backend.m_draft); saved["revision"] = 7;
        differing["revision"] = 27; differing["local_revision"] = 7; saved["google"] = differing;
        backend.m_draftDirty = false; startRead(); finishRead(saved);
        statusText = named(root, "googleDraftStatus")->property("text").toString();
        require(statusText.contains("Newer local edits await Google draft sync") && statusText.contains("differs from your local version")
            && !statusText.contains("Unsaved edits"), "cached saved revision keeps both pending Google sync and fidelity warning visible");

        // A selected old operation can arrive with today's twin revision. Its
        // historical state must not supersede the current operation or editor.
        setStatus(status("conflict", 28, 7)); captured = openReplacement(); expectReplacement(captured);
        click("confirmReplaceGoogleDraft"); waitOperation(7);
        localEdit("New work while the earlier receipt is held"); const auto duringReplay = authored(backend.m_draft);
        setStatus(status("prepared", 29, 7));
        auto historical = status("synced", 29, 7); historical["operation_is_current"] = false;
        historical["operation"] = QJsonObject{{"request_id", captured["request_id"]}, {"state", "synced"}};
        const auto expectedRefresh = reads + 1; complete(historical);
        await([&] { return reads == expectedRefresh; }, "historical auxiliary reply requests current cached metadata");
        require(backend.m_draft["google"].toMap()["state"] == "prepared" && authored(backend.m_draft) == duringReplay,
            "same-revision historical state never replaces current metadata or authored fields");
        finishRead(projection(8, status("prepared", 30, 8)));
        require(backend.m_draft["google"].toMap()["revision"].toLongLong() == 30 && authored(backend.m_draft) == duringReplay
            && backend.m_draftDirty && fixture.operations == 7, "historical recovery merges cached metadata only without replay or autosave");
        historical["revision"] = 99; auto staleProjection = projection(9, historical);
        require(backend.mergedDraftProjection(staleProjection)["google"] == backend.m_draft["google"], "ordinary projection merge also refuses historical Google receipts");

        const auto unknownConflict = [&](qint64 revision, const QJsonValue &target, const QJsonValue &observed) {
            auto value = status("conflict", revision, 7);
            if (target.isNull()) value["baseline"] = QJsonValue::Null;
            value["operation"] = QJsonObject{{"request_id", "unknown-original"}, {"state", "acceptance_unknown"}, {"provider_draft_id", target}};
            value["conflict"] = QJsonObject{{"reason", "acceptance_unknown"}, {"observed", observed}};
            return value;
        };
        backend.m_draftDirty = false; emit backend.draftChanged();
        prepareCheck(); click("checkGoogleDraft"); waitOperation(8);
        complete(unknownConflict(31, QJsonValue::Null, QJsonValue::Null));
        require(!backend.googleDraft()["can_replace"].toBool() && !backend.googleDraft()["can_check"].toBool()
            && named(root, "googleDraftStatus")->property("text").toString().contains("no known target"), "unknown creation exposes no unsafe action or read without a known identity");
        setStatus(unknownConflict(32, "provider-draft", QJsonValue::Null));
        require(!backend.googleDraft()["can_replace"].toBool() && named(root, "googleDraftStatus")->property("text").toString().contains("currently missing"),
            "missing unknown update cannot create a replacement and does not imply delivery");
        QJsonObject observed{{"draft_id", "foreign-provider-draft"}, {"message_id", "observed-message"}, {"sha256", QString(64, 'c')}, {"size", 200}};
        setStatus(unknownConflict(33, "provider-draft", observed));
        require(!backend.googleDraft()["can_replace"].toBool(), "unknown update rejects a different observed target");
        auto pendingUnknown = unknownConflict(34, "provider-draft", QJsonValue::Null); pendingUnknown["state"] = "acceptance_unknown";
        setStatus(pendingUnknown); click("checkGoogleDraft"); waitOperation(9);
        observed["draft_id"] = "provider-draft";
        complete(unknownConflict(35, "provider-draft", observed));
        require(backend.googleDraft()["can_replace"].toBool(), "fresh matching unknown-update observation permits explicit replacement");
        captured = openReplacement(); expectReplacement(captured);
        require(captured["request_id"] != "unknown-original" && captured["expected_revision"] == 7 && captured["expected_google_revision"] == 35,
            "unknown-update recovery captures new request identity and exact current revisions");
        click("confirmReplaceGoogleDraft"); waitOperation(10); complete(status("acknowledged", 36, 7));
        require(fixture.operations == 10 && authored(backend.m_draft) == duringReplay, "unknown-update recovery performs one explicit attempt and preserves local work");

        auto unprepared = status("failed", 37, 7); unprepared["baseline"] = QJsonValue::Null;
        unprepared["preparation_failure"] = QJsonObject{{"draft_revision", 7}, {"code", "invalid_draft"}};
        setStatus(unprepared); statusText = named(root, "googleDraftStatus")->property("text").toString();
        require(statusText.contains("not uploaded") && statusText.contains("addresses, formatting or attachments") && statusText.contains("Edit and save to retry")
            && !backend.googleDraft()["can_check"].toBool() && !statusText.contains("Check Google"), "local preparation failure states its reason without claiming a remote copy");
        localEdit("Edited to correct the local preparation failure");
        require(named(root, "googleDraftStatus")->property("text").toString().contains("Unsaved edits await local save"), "failed draft shows edited unsaved work awaiting retry");
        auto corrected = QJsonObject::fromVariantMap(backend.m_draft); corrected["revision"] = 8;
        unprepared["revision"] = 38; unprepared["local_revision"] = 8; corrected["google"] = unprepared;
        backend.m_draftDirty = false; startRead(); finishRead(corrected);
        statusText = named(root, "googleDraftStatus")->property("text").toString();
        require(statusText.contains("Newer saved edits await preparation") && !statusText.contains("Unsaved edits"), "new saved revision shows preparation pending while retaining the prior failure reason");
        auto priorRemote = status("failed", 39, 8);
        priorRemote["preparation_failure"] = QJsonObject{{"draft_revision", 8}, {"code", "limit_exceeded"}};
        setStatus(priorRemote); statusText = named(root, "googleDraftStatus")->property("text").toString();
        require(statusText.contains("35,000,000-byte") && backend.googleDraft()["can_check"].toBool() && fixture.operations == 10,
            "oversize local revision can inspect a retained known remote baseline without automatically uploading");
        require(!qmlError, "native draft controls generated no QML errors");
        backend.m_draftTimer.stop(); backend.m_pollTimer.stop();
        qInfo() << "Native Google draft controls passed: actual QML recovery/confirmation, private held IPC, projection revision floors, authored-text/account isolation, historical receipt isolation, exact captured unknown-update recovery, unknown/cancel readback and visible byte mismatch. Synthetic IPC only; no provider or performance claim.";
    }
};

int main(int argc, char **argv) {
    QQuickStyle::setStyle("Basic"); QGuiApplication app(argc, argv);
    GoogleDraftControlsTest::run(); return 0;
}
