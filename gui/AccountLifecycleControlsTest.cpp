#include "GoogleOperationTestFixture.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QGuiApplication>
#include <QFontDatabase>
#include <QJsonArray>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickStyle>
#include <QQuickWindow>
using namespace GoogleOperationFixture;
namespace {
QJsonObject account(const QString &id, int revision = 1, const QString &state = "active", const QString &cleanup = {}) {
    QJsonObject life{{"state", state}, {"revision", revision}, {"cleanup", QJsonValue::Null}};
    if (!cleanup.isEmpty()) life["cleanup"] = QJsonObject{{"state", cleanup}, {"request_id", "prior-remove"}, {"code", QJsonValue::Null}};
    return {{"id", id}, {"name", id}, {"email", id + "@example.test"}, {"lifecycle", life},
        {"google", QJsonObject{{"capabilities", QJsonObject{{"mail_read", true}, {"mail_write", true}, {"mail_delivery", true}, {"send", true}}}}}};
}
QJsonObject draft() {
    return {{"account", "fixture-account"}, {"draft_id", "authored-one"}, {"mode", "new"}, {"origin", "local"},
        {"revision", 1}, {"body", "Saved original"}, {"body_html", QJsonValue::Null}, {"subject", "Retained subject"},
        {"from", "fixture-account@example.test"}, {"from_name", "Original name"}, {"signature_text", "Saved signature"},
        {"attachment_ids", QJsonArray{}}, {"attachments", QJsonArray{}}, {"to_recipients", QJsonArray{QJsonObject{{"email", "guest@example.test"}, {"name", "Guest"}}}},
        {"cc_recipients", QJsonArray{}}, {"bcc_recipients", QJsonArray{}}, {"google", QJsonValue::Null}, {"google_send", QJsonValue::Null}};
}
QQuickItem *named(QQuickItem *root, const QString &name) {
    if (root->objectName() == name) return root;
    for (auto child : root->childItems()) if (auto found = named(child, name)) return found;
    return nullptr;
}
}
class AccountLifecycleControlsTest {
public:
static void run() {
    Engine fixture; MailBackend backend(fixture.profile, "unused", nullptr, false, true);
    backend.m_connected = true; backend.m_syntheticProfile = false; backend.m_account = "fixture-account";
    backend.m_activeDraft = "authored-one"; backend.m_draft = draft().toVariantMap();
    backend.m_enginePid = QCoreApplication::applicationPid(); backend.m_generation = 1;
    QJsonArray rows{account("fixture-account"), account("second-account")}; QJsonObject saved = draft();
    backend.m_allAccounts = rows.toVariantList(); backend.publishAccounts(rows.toVariantList()); backend.reconcileAccountLifecycle();
    backend.m_preferences = {{"theme", "graphite"}, {"text_size", "default"}, {"calendar_display_time_zone", "UTC"}};
    QObject::disconnect(&backend.m_socket, SIGNAL(connected()), &backend, nullptr);
    backend.m_socket.connectToServer(fixture.server.serverName());
    await([&] { return backend.m_socket.state() == QLocalSocket::ConnectedState; }, "cached socket");
    bool hello = false; backend.request("hello", {}, {{"token", "synthetic-operation-token"}}, [&](auto, bool ok) { hello = ok; }); await([&] { return hello; }, "cached authentication");
    QPointer<QLocalSocket> held; QString heldId; QJsonObject heldParams; bool holdSave = false;
    QPointer<QLocalSocket> heldView, heldReload; QString heldViewId, heldReloadId; bool holdView = false, holdReload = false, conflictViewSave = false;
    int saves = 0, previews = 0, removals = 0, blockedWrites = 0; QJsonObject removalRequest, lastReceipt;
    fixture.customRequest = [&](QLocalSocket *peer, const QJsonObject &request) {
        const auto method = request["method"].toString(), id = request["id"].toString(), selected = request["account"].toString();
        const auto params = request["params"].toObject(); QJsonObject result;
        if (method == "accounts") { require(params["include_removed"].toBool(), "explicit archive discovery"); result["accounts"] = rows; }
        else if (method == "draft_save") {
            ++saves; if (holdSave) { held = peer; heldId = id; heldParams = params; return true; }
            saved["body"] = params["body"]; saved["revision"] = saved["revision"].toInt() + 1; result = saved;
        } else if (method == "draft_get") {
            if (holdReload) { heldReload = peer; heldReloadId = id; return true; }
            result = saved;
            if (backend.accountRemoved(selected)) { result["account_archived"] = true; result["read_only"] = true; }
        } else if (method == "account_remove_preview") {
            ++previews; QJsonObject selectedRow;
            for (const auto &row : rows) if (row.toObject()["id"] == selected) selectedRow = row.toObject();
            result = {{"account", selected}, {"lifecycle", selectedRow["lifecycle"]}, {"retention", "archive"}, {"read_only", true}};
        } else if (method == "account_remove") {
            ++removals; removalRequest = params; const auto revision = params["expected_revision"].toInt() + 1;
            const auto state = removals == 1 ? "failed" : "complete";
            rows[0] = account("fixture-account", revision, "removed", state);
            auto life = rows[0].toObject()["lifecycle"].toObject(); auto cleanup = life["cleanup"].toObject(); cleanup["request_id"] = params["request_id"]; life["cleanup"] = cleanup;
            auto row = rows[0].toObject(); row["lifecycle"] = life; rows[0] = row;
            lastReceipt = {{"account", selected}, {"request_id", params["request_id"]}, {"operation_is_current", true}, {"lifecycle", life},
                {"operation", QJsonObject{{"kind", "account_remove"}, {"account", selected}, {"request_id", params["request_id"]}, {"retention", "archive"}, {"removed_revision", revision}, {"connection_generation", revision}}}};
            result = lastReceipt;
        } else if (method == "account_remove_status") result = lastReceipt;
        else if (method == "view_get") {
            if (holdView) { heldView = peer; heldViewId = id; return true; }
            result = {{"view", QJsonObject{}}, {"revision", 1}};
        }
        else if (method == "drafts") result = {{"drafts", QJsonArray{saved}}, {"total", 1}};
        else if (method == "categories") result["categories"] = QJsonArray{};
        else if (method == "conversations") result = {{"conversations", QJsonArray{}}, {"total", 0}};
        else if (method == "capture_history") result = {{"captures", QJsonArray{}}, {"total", 0}};
        else if (method == "calendar_combined") result = {{"events", QJsonArray{}}, {"time_zone", "UTC"}, {"coverage", QJsonObject{{"selected_calendars", 0}, {"complete", true}}}};
        else if (method == "calendar_sources") result["calendars"] = QJsonArray{};
        else if (method == "google_drafts_status") result = {{"account", selected}, {"generation", 0}};
        else if (method == "view_save") {
            if (conflictViewSave) {
                conflictViewSave = false;
                peer->write(QJsonDocument(QJsonObject{{"version", 1}, {"id", id}, {"ok", false},
                    {"error", QJsonObject{{"code", "stale_revision"}, {"message", "View changed."}}}}).toJson(QJsonDocument::Compact) + '\n');
                return true;
            }
            result["revision"] = 2;
        }
        else { ++blockedWrites; result = {}; }
        peer->write(reply(id, result)); return true;
    };
    QTemporaryDir copy; require(copy.isValid(), "QML staging"); const auto modulePath = copy.path() + "/LifecycleTest";
    require(QDir().mkpath(modulePath), "QML directory");
    QFile module(modulePath + "/qmldir"); require(module.open(QIODevice::WriteOnly), "QML metadata"); module.write("module LifecycleTest\n");
    for (const auto &file : QDir(QString::fromUtf8(FM_GUI_SOURCE_DIR)).entryList({"*.qml"}, QDir::Files)) {
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + '/' + file, modulePath + '/' + file), "production QML copy");
        const auto name = QFileInfo(file).completeBaseName(); module.write(((name == "Theme" ? "singleton " : "") + name + " 1.0 " + file + '\n').toUtf8());
    }
    module.close();
    for (const auto font : {"Geist-Regular.ttf", "Geist-Medium.ttf", "Geist-SemiBold.ttf"}) require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/" + font) >= 0, "bundled fonts");
    qmlRegisterType<ReplyDocument>("LifecycleTest", 1, 0, "ReplyDocument");
    QQmlApplicationEngine qml; bool warnings = false;
    QObject::connect(&qml, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { warnings = true; for (const auto &error : errors) qWarning() << error; });
    qml.addImportPath(copy.path()); qml.rootContext()->setContextProperty("backend", &backend);
    qml.loadData("import QtQuick\nimport QtQuick.Controls\nimport LifecycleTest\nApplicationWindow { id: testWindow; width: 1000; height: 720; visible: true; color: Theme.canvas; property bool settingsShown: false; function appearance(theme,size) { Theme.themeId=theme; Theme.textSize=size } AccountLifecycleDialogs {} ScrollView { visible: !testWindow.settingsShown; anchors.fill: parent; ReplyComposer { width: 440 } } SettingsView { visible: testWindow.settingsShown; anchors.fill: parent; page: 'Accounts' } }");
    require(!qml.rootObjects().isEmpty(), "production lifecycle dialogs and composer load"); auto window = qobject_cast<QQuickWindow *>(qml.rootObjects().first());
    auto activate = [&](const QString &name) { auto item = named(window->contentItem(), name); require(item && item->isVisible() && item->isEnabled(), qPrintable("available control " + name)); require(QMetaObject::invokeMethod(item, "clicked"), "actual clicked handler"); };
    QVariantMap confirmation; QObject::connect(&backend, &MailBackend::accountRemovalReady, [&](const QVariantMap &value) { confirmation = value; });
    backend.stageDraftRecipient("to_recipients", "invalid-address"); backend.previewAccountRemoval("fixture-account");
    require(previews == 0 && backend.recipientInputs().value("to_recipients") == "invalid-address", "incomplete inputs retained before removal");
    backend.stageDraftRecipient("to_recipients", ""); backend.editDraft("Authored before remove"); holdSave = true;
    backend.previewAccountRemoval("fixture-account"); await([&] { return held; }, "real save is held before preview");
    require(previews == 0 && confirmation.isEmpty(), "confirmation waits for durable save");
    saved["body"] = heldParams["body"]; saved["revision"] = 2; held->write(reply(heldId, saved)); held.clear(); holdSave = false;
    await([&] { return !confirmation.isEmpty(); }, "fresh preview after successful save");
    require(saved["body"] == "Authored before remove" && confirmation.value("lifecycle").toMap().value("revision").toInt() == 1, "saved text plus current lifecycle");
    backend.editDraft("Changed after confirmation"); activate("confirmAccountRemoval");
    require(removals == 0 && backend.m_draftDirty, "changed editor invalidates removal confirmation");
    confirmation.clear(); backend.previewAccountRemoval("fixture-account"); await([&] { return !confirmation.isEmpty(); }, "new save and confirmation");
    activate("confirmAccountRemoval"); await([&] { return backend.account() == "second-account" && backend.accountRemoved("fixture-account"); }, "selected account removal chooses remaining active tab");
    require(backend.accounts().size() == 1 && backend.allAccounts().size() == 2 && saved["body"] == "Changed after confirmation", "saved archive stays distinct from active inventory");
    const auto firstRequest = removalRequest["request_id"].toString(); backend.retryAccountCleanup("fixture-account");
    await([&] { return removals == 2 && !backend.m_accountRemovalBusy.contains("fixture-account"); }, "failed cleanup retry completes");
    require(removalRequest["request_id"].toString() != firstRequest && removalRequest["expected_revision"].toInt() == 2, "cleanup retry uses fresh identity and current lifecycle");
    await([&] { return backend.m_pending.isEmpty(); }, "cleanup cached publication");
    const auto completedRequest = removalRequest["request_id"].toString();
    // A failed restore may have stored a credential. The old completed removal
    // receipt must not be replayed as authority to delete that credential.
    auto afterRestore = account("fixture-account", 4, "removed", "pending");
    auto restoreLife = afterRestore["lifecycle"].toObject(); auto restoreCleanup = restoreLife["cleanup"].toObject();
    restoreCleanup["request_id"] = completedRequest; restoreCleanup["replay_cleanup"] = false; restoreLife["cleanup"] = restoreCleanup; afterRestore["lifecycle"] = restoreLife;
    rows[0] = afterRestore; backend.loadAccounts(); await([&] { return backend.m_pending.isEmpty(); }, "failed restore cleanup publication");
    backend.retryAccountCleanup("fixture-account"); await([&] { return removals == 3 && backend.m_pending.isEmpty(); }, "restore cleanup retry");
    require(removalRequest["request_id"].toString() != completedRequest && removalRequest["expected_revision"].toInt() == 4, "failed restore needs fresh cleanup authority");
    backend.openAccountArchive("fixture-account"); await([&] { return backend.account() == "fixture-account" && backend.m_pending.isEmpty(); }, "explicit archived account selection");
    backend.openDraft("authored-one"); await([&] { return !backend.draftBusy() && !backend.m_draft.isEmpty(); }, "saved archived draft opens");
    require(backend.accountArchived() && backend.draftLocked() && !backend.draftReadOnly(), "account archive is separate from Google snapshot provenance");
    const auto archived = backend.draft(); const auto savesBefore = saves, refusedBefore = blockedWrites;
    backend.editDraft("no"); backend.setDraftSubject("no"); backend.stageDraftRecipient("cc_recipients", "no@example.test"); backend.removeDraftRecipient("to_recipients", 0);
    backend.setDraftSignature("no"); backend.editFormattedDraft("no", "<b>no</b>"); backend.discardDraft(); backend.captureDraft(); backend.beginDraft("new"); backend.sendGoogleDraft(); backend.resumeGoogleSend(); backend.saveMyDraftVersion(); backend.saveDraft();
    require(backend.draft() == archived && saves == savesBefore && blockedWrites == refusedBefore && !backend.m_draftTimer.isActive(), "all local authoring routes blocked without IPC");
    auto editor = named(window->contentItem(), "replyComposer"); require(editor && editor->property("readOnly").toBool() && editor->isEnabled(), "archive text remains selectable");
    // A lifecycle publication may race a queued completion from any transport.
    // Exercise the actual draft-save callback, delivered after removal is known.
    rows[0] = account("fixture-account", 6); backend.m_allAccounts = rows.toVariantList(); backend.publishAccounts(rows.toVariantList()); backend.reconcileAccountLifecycle();
    backend.m_draft = saved.toVariantMap(); backend.m_activeDraft = "authored-one"; backend.m_archiveSelection.clear();
    backend.editDraft("Unsaved original-account buffer"); backend.stageDraftRecipient("bcc_recipients", "unfinished"); backend.m_recipientInputs.clear(); holdSave = true; backend.saveDraft();
    await([&] { return held; }, "held native save for removal race"); backend.m_recipientInputs["bcc_recipients"] = "unfinished";
    const auto unsaved = backend.draft(), pendingRecipients = backend.recipientInputs();
    rows[0] = account("fixture-account", 7, "removed", "complete"); backend.m_allAccounts = rows.toVariantList(); backend.publishAccounts({rows[1].toVariant()}); backend.reconcileAccountLifecycle();
    require(backend.unsavedAccountBuffer() && backend.account() == "fixture-account" && backend.draft() == unsaved, "removed account retains original dirty buffer identity");
    held->write(reply(heldId, saved)); held.clear(); holdSave = false; await([&] { return backend.m_pending.isEmpty(); }, "late old save arrives");
    require(backend.draft() == unsaved && backend.recipientInputs() == pendingRecipients && backend.m_draftDirty, "late save cannot overwrite fields or acknowledge dirty buffer");
    backend.selectAccount("second-account"); activate("keepRemovedAccountBuffer");
    require(backend.account() == "fixture-account" && backend.unsavedAccountBuffer(), "Keep here cancels navigation without stranding buffer");
    backend.shutdown(); require(!backend.m_shutdown && backend.unsavedAccountBuffer(), "close requires explicit unsaved disposition"); activate("keepRemovedAccountBuffer");
    require(!backend.accountNavigationAllowed(), "window navigation also requires disposition"); activate("keepRemovedAccountBuffer");
    backend.openDraft("authored-one"); activate("discardRemovedAccountBuffer"); await([&] { return backend.account() == "fixture-account" && backend.activeDraftId() == "authored-one" && !backend.draftBusy() && backend.m_pending.isEmpty(); }, "same-account draft continuation keeps archive scope");
    require(saved["body"] == "Changed after confirmation" && !backend.unsavedAccountBuffer(), "saved draft untouched by buffer discard");
    rows[0] = account("fixture-account", 8); backend.m_allAccounts = rows.toVariantList(); backend.publishAccounts(rows.toVariantList()); backend.reconcileAccountLifecycle();
    await([&] { return backend.m_pending.isEmpty(); }, "restored account navigation"); backend.openDraft("authored-one"); await([&] { return !backend.draftBusy(); }, "restored draft open");
    backend.editDraft("Before unseen remove and restore"); holdSave = true; backend.saveDraft(); await([&] { return held; }, "old-epoch save held");
    backend.m_afterDraftSave = [&] { ++blockedWrites; }; backend.m_googleSendPending = {{"request_id", "must-not-continue"}};
    const auto epochBuffer = backend.draft(); rows[0] = account("fixture-account", 10);
    backend.m_allAccounts = rows.toVariantList(); backend.publishAccounts(rows.toVariantList()); backend.reconcileAccountLifecycle();
    require(backend.unsavedAccountBuffer() && !backend.m_savingDraft && !backend.m_draftTimer.isActive()
        && !backend.m_afterDraftSave && backend.m_googleSendPending.isEmpty(), "unseen removal/restore invalidates busy state and all authored continuations");
    held->write(reply(heldId, saved)); held.clear(); holdSave = false; await([&] { return backend.m_pending.isEmpty(); }, "old-epoch callback drains");
    require(backend.draft() == epochBuffer && backend.m_draftDirty && !backend.draftBusy(), "old-epoch success cannot acknowledge text or wedge busy state");
    backend.selectAccount("second-account"); activate("discardRemovedAccountBuffer"); await([&] { return backend.account() == "second-account" && backend.m_pending.isEmpty(); }, "explicit navigation after unseen epoch change");
    // Restore passively rehydrates the saved view before resolving its draft.
    // The actual Remove handler must survive that generation change, while
    // still waiting for the exact draft to finish loading before confirmation.
    confirmation.clear(); holdView = true; backend.selectAccount("fixture-account");
    await([&] { return heldView; }, "restored view is held before draft resolution");
    emit backend.accountsChanged(); window->setProperty("settingsShown", true); QCoreApplication::processEvents(); activate("removeAccount_fixture-account");
    const auto removalsBeforeRestorePreview = removals;
    holdView = false; holdReload = true;
    heldView->write(reply(heldViewId, {{"view", QJsonObject{{"selectedDraft", "authored-one"}, {"folder", "drafts"}}}, {"revision", 1}})); heldView.clear();
    await([&] { return heldReload; }, "restored draft is held after view publication");
    require(confirmation.isEmpty() && backend.draftBusy(), "removal confirmation waits for restored draft");
    holdReload = false; heldReload->write(reply(heldReloadId, saved)); heldReload.clear();
    await([&] { return backend.m_pending.isEmpty(); }, "restored view and draft replies drain");
    require(!confirmation.isEmpty() && confirmation.value("ui_generation").toInt() == backend.m_generation,
        "Remove click survives passive restored-view generation change");
    require(removals == removalsBeforeRestorePreview && backend.draft().value("body") == saved["body"].toVariant(), "deferred preview never removes or authors automatically");
    activate("cancelAccountRemoval"); window->setProperty("settingsShown", false);
    backend.selectAccount("second-account"); await([&] { return backend.m_pending.isEmpty(); }, "restore preview cleanup");
    // Removing another Settings account is also independent of the selected
    // mailbox's passive restore, but the target lifecycle remains exact.
    confirmation.clear(); holdView = true; backend.selectAccount("fixture-account"); await([&] { return heldView; }, "other-target view hold");
    window->setProperty("settingsShown", true); QCoreApplication::processEvents(); activate("removeAccount_second-account");
    holdView = false; heldView->write(reply(heldViewId, {{"view", QJsonObject{{"selectedDraft", "authored-one"}}}, {"revision", 1}})); heldView.clear();
    await([&] { return backend.m_pending.isEmpty(); }, "other-target passive restore");
    require(confirmation.value("account") == "second-account" && backend.account() == "fixture-account", "passive restore preserves the explicit other-account target");
    rows[1] = account("second-account", 2); backend.m_allAccounts = rows.toVariantList(); emit backend.accountsChanged();
    activate("confirmAccountRemoval"); require(removals == removalsBeforeRestorePreview && backend.error() == "The account changed. Review removal again.", "stale target lifecycle refuses removal");
    for (const bool switchAccount : {true, false}) {
        backend.clearError(); confirmation.clear(); backend.selectAccount("second-account"); await([&] { return backend.m_pending.isEmpty(); }, "navigation-refusal setup");
        holdView = true; backend.selectAccount("fixture-account"); await([&] { return heldView; }, "view held before deliberate navigation");
        activate("removeAccount_fixture-account"); const auto previewsBeforeNavigation = previews;
        if (switchAccount) backend.selectAccount("second-account"); else backend.openDraft("authored-one");
        holdView = false; heldView->write(reply(heldViewId, {{"view", QJsonObject{{"selectedDraft", "authored-one"}}}, {"revision", 1}})); heldView.clear();
        await([&] { return backend.m_pending.isEmpty(); }, "old view drains after deliberate navigation");
        require(confirmation.isEmpty() && previews == previewsBeforeNavigation && removals == removalsBeforeRestorePreview
            && backend.error() == "The account or editor changed. Review removal again.", "deliberate account/draft navigation refuses deferred removal visibly");
    }
    backend.clearError(); confirmation.clear(); backend.selectAccount("second-account"); await([&] { return backend.m_pending.isEmpty(); }, "conflict-view setup");
    holdView = true; conflictViewSave = true; backend.remember("readerY", 12); backend.saveView("second-account");
    await([&] { return heldView; }, "view-save conflict recovery is held");
    activate("removeAccount_fixture-account");
    if (!backend.m_deferredAccountRemovalPreview.isEmpty()) qInfo() << "Conflict view state" << backend.m_generation << backend.m_accountViewRestorationGeneration << backend.m_deferredAccountRemovalPreview << conflictViewSave;
    require(backend.m_deferredAccountRemovalPreview.isEmpty(), "metadata-only view recovery does not own deferred Remove intent");
    holdView = false; heldView->write(reply(heldViewId, {{"view", QJsonObject{}}, {"revision", 2}})); heldView.clear();
    await([&] { return backend.m_pending.isEmpty(); }, "conflict view and queued removal preview drain");
    require(confirmation.value("account") == "fixture-account" && removals == removalsBeforeRestorePreview, "Remove proceeds after non-navigation view recovery without automatic removal");
    activate("cancelAccountRemoval"); window->setProperty("settingsShown", false);
    rows[0] = account("fixture-account", 11, "removed", "complete");
    rows[1] = account("second-account", 2, "removed", "complete"); backend.m_allAccounts = rows.toVariantList(); backend.publishAccounts({}); backend.reconcileAccountLifecycle();
    require(backend.account().isEmpty() && backend.draft().isEmpty() && backend.conversations().isEmpty(), "last account removal clears active workspace");
    const auto captureDir = qEnvironmentVariable("FM_ACCOUNT_LIFECYCLE_CAPTURES");
    if (!captureDir.isEmpty()) {
        require(QDir().mkpath(captureDir), "capture directory"); window->setProperty("settingsShown", true);
        emit backend.accountsChanged();
        for (const auto theme : {"graphite", "paper", "harbour"}) for (const auto size : {"default", "large"}) {
            require(QMetaObject::invokeMethod(window, "appearance", Q_ARG(QVariant, QString(theme)), Q_ARG(QVariant, QString(size))), "appearance");
            QCoreApplication::processEvents();
            const auto settingsPath = QDir(captureDir).filePath(QString("accounts-%1-%2.png").arg(theme, size));
            require(!QFileInfo::exists(settingsPath) && window->grabWindow().save(settingsPath), "retained Settings capture");
            auto context = confirmation; context["email"] = "long.account.identity.for.retained.work@workspace.example.test";
            emit backend.accountRemovalReady(context); QCoreApplication::processEvents();
            const auto confirm = named(window->contentItem(), "confirmAccountRemoval");
            require(confirm && confirm->isVisible() && QRectF(0, 0, window->width(), window->height()).contains(confirm->mapRectToScene(confirm->boundingRect())), "removal confirmation remains contained");
            const auto dialogPath = QDir(captureDir).filePath(QString("remove-%1-%2.png").arg(theme, size));
            require(!QFileInfo::exists(dialogPath) && window->grabWindow().save(dialogPath), "retained removal capture"); activate("cancelAccountRemoval");
        }
    }
    require(!warnings, "no QML warnings");
    backend.m_connected = false; backend.m_socket.abort();
    qInfo() << "Account lifecycle native handlers, archive guards, fresh confirmation, cleanup retry and unsaved callback/close/navigation fences passed.";
}
};
int main(int argc, char **argv) { QQuickStyle::setStyle("Basic"); QGuiApplication app(argc, argv); AccountLifecycleControlsTest::run(); return 0; }
