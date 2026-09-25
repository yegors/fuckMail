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
        const auto hint = [&] { return named(root, "composerHint")->property("text").toString(); };
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

        // Drafts are Gmail drafts (D122): syncing and conflicts resolve on their
        // own; only a problem the person should know about reaches the hint line.
        setStatus(status("synced", 11, 6));
        require(!backend.googleDraft()["attention"].toBool() && !hint().contains("Gmail"), "normal Gmail draft sync is silent in the composer");
        setStatus(status("conflict", 12, 6));
        require(!backend.googleDraft()["attention"].toBool() && !hint().contains("Gmail"), "a conflict resolves without asking");
        for (const auto name : {"checkGoogleDraft", "replaceGoogleDraft", "googleDraftReplaceDialog", "draftSourceHistoryButton", "draftSourceActionsButton", "collapseReply"})
            require(!named(root, name), "no manual sync, source or minimise controls remain");
        auto unknownCreate = status("acceptance_unknown", 13, 6); unknownCreate["baseline"] = QJsonValue::Null;
        unknownCreate["operation"] = QJsonObject{{"request_id", "unknown-original"}, {"state", "acceptance_unknown"}, {"provider_draft_id", QJsonValue::Null}};
        setStatus(unknownCreate);
        require(hint().contains("Gmail hasn't confirmed") && !backend.googleDraft()["warning"].toBool(), "an unconfirmed create says so plainly, without alarm");
        setStatus(status("failed", 14, 6));
        require(hint().contains("Couldn't save this draft to Gmail") && backend.googleDraft()["warning"].toBool(), "a stopped sync says so plainly");
        auto unprepared = status("failed", 15, 6); unprepared["baseline"] = QJsonValue::Null;
        unprepared["preparation_failure"] = QJsonObject{{"draft_revision", 6}, {"code", "limit_exceeded"}};
        backend.m_draftDirty = false; setStatus(unprepared);
        require(hint().contains("35 MB"), "a draft Gmail cannot take states the reason");
        localEdit("Edited to fix the size");
        require(!hint().contains("35 MB") && hint().contains("Couldn't save"), "an edit after a failure drops the stale reason until the next upload");
        // An old operation's receipt never replaces current metadata.
        auto historical = status("synced", 99, 6); historical["operation_is_current"] = false;
        historical["operation"] = QJsonObject{{"request_id", "old"}, {"state", "synced"}};
        require(backend.mergedDraftProjection(projection(9, historical))["google"] == backend.m_draft["google"], "ordinary projection merge refuses historical Google receipts");
        require(!qmlError, "native draft controls generated no QML errors");
        backend.m_draftTimer.stop(); backend.m_pollTimer.stop();
        qInfo() << "Native Google draft controls passed: silent sync, plain problem hints, no manual recovery controls, private held IPC, projection revision floors, authored-text/account isolation and historical receipt isolation. Synthetic IPC only; no provider or performance claim.";
    }
};

int main(int argc, char **argv) {
    QQuickStyle::setStyle("Basic"); QGuiApplication app(argc, argv);
    GoogleDraftControlsTest::run(); return 0;
}
