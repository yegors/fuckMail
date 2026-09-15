#include "GuiAccountLifecycleSelfTest.h"
#include "MailBackend.h"
#include <QCoreApplication>
#include <QDesktopServices>
#include <QDate>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QInputMethodEvent>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QProcess>
#include <QQuickItem>
#include <QQuickTextDocument>
#include <QQuickWindow>
#include <QSaveFile>
#include <QTextDocument>
#include <QUuid>
#include <QUrlQuery>
namespace {
const QStringList quotaKeys{"gmail_project_units_per_minute", "gmail_account_units_per_minute", "calendar_project_units_per_minute", "calendar_account_units_per_minute"};
QQuickItem *named(QQuickItem *root, const QString &name) {
    if (!root) return nullptr;
    if (root->objectName() == name) return root;
    for (auto child : root->childItems()) if (auto found = named(child, name)) return found;
    return nullptr;
}
QJsonObject unread(const QJsonObject &conversation) {
    QJsonObject result;
    for (const auto &value : conversation["messages"].toArray()) { const auto message = value.toObject(); result[message["id"].toString()] = message["unread"]; }
    return result;
}
}
GuiAccountLifecycleSelfTest::GuiAccountLifecycleSelfTest(MailBackend &backend, QQuickWindow *window,
        const QString &expectation, const QString &report, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_report(report) {
    m_elapsed.start();
    if (QFileInfo::exists(report)) { m_finished = true; QTimer::singleShot(0, this, [] { QCoreApplication::exit(5); }); return; }
    QFile input(expectation);
    if (!input.open(QIODevice::ReadOnly) || input.size() > 1024 * 1024) { QTimer::singleShot(0, this, [this] { finish("Missing or oversized lifecycle expectation."); }); return; }
    const auto expected = QJsonDocument::fromJson(input.readAll()).object();
    m_cli = expected["cli_path"].toString(); m_limits = expected["limits"].toObject(); m_expected = expected["accounts"].toArray(); m_date = expected["calendar_date"].toString();
    if (!window || expected["version"] != 1 || m_expected.size() != 2 || m_limits.size() != 4 || !QFileInfo(m_cli).isAbsolute() || !QDate::fromString(m_date, Qt::ISODate).isValid()) {
        QTimer::singleShot(0, this, [this] { finish("Invalid lifecycle expectation identity/paths."); }); return;
    }
    for (int index = 0; index < 2; ++index) {
        const auto row = m_expected[index].toObject(), draft = row["draft"].toObject();
        if (row["identity"] != (index == 0 ? "personal" : "workspace") || row["account"].toString().isEmpty()
            || row["email"].toString().isEmpty() || row["renamed_email"].toString().isEmpty()
            || draft["subject"].toString().isEmpty() || draft["body"].toString().isEmpty() || draft["dirty_body"].toString().isEmpty()
            || draft["recipient"].toString().isEmpty() || (index == 1 && draft["pending_recipient"].toString().isEmpty())) {
            QTimer::singleShot(0, this, [this] { finish("Incomplete generated account/draft expectation."); }); return;
        }
    }
    for (const auto &key : quotaKeys) if (m_limits[key].toInteger() < 100 || m_limits[key].toInteger() > 100000000) {
        QTimer::singleShot(0, this, [this] { finish("Invalid bounded fixture sync limits."); }); return;
    }
    connect(&backend, &MailBackend::accountRemovalReady, this, [this](const QVariantMap &context) { m_confirmation = QJsonObject::fromVariantMap(context); });
    window->resize(1280, 900); window->setProperty("workspace", "mail"); window->setProperty("settingsOpen", false);
    m_timer.setInterval(50); connect(&m_timer, &QTimer::timeout, this, &GuiAccountLifecycleSelfTest::tick);
    m_watchdog.setSingleShot(true); connect(&m_watchdog, &QTimer::timeout, this, [this] { finish(QString("Native account lifecycle timed out at stage %1, account %2.").arg(m_stage).arg(m_account)); });
    m_watchdog.start(120000); m_timer.start();
}
GuiAccountLifecycleSelfTest::~GuiAccountLifecycleSelfTest() { if (m_probe) QDesktopServices::unsetUrlHandler("https"); }
QJsonObject GuiAccountLifecycleSelfTest::diagnosticState() const {
    const auto id = m_account >= 0 && m_account < m_expected.size() ? m_expected[m_account].toObject()["account"].toString() : QString{};
    const auto life = m_backend.lifecycleAccount(id).value("lifecycle").toMap();
    const auto ui = m_backend.m_accountLifecycleUi.value(id).toMap();
    QJsonArray pending;
    for (const auto &request : m_backend.m_pending) if (request.method == "account_remove_preview" && pending.size() < 16)
        pending.append(QJsonObject{{"account", request.account}, {"generation", request.generation}});
    QJsonObject popups;
    for (const auto &name : {"googleSignInDialog", "accountRemoveDialog", "removedAccountBufferDialog"}) {
        auto popup = m_window ? m_window->findChild<QObject *>(name) : nullptr;
        QJsonObject state{{"exists", popup != nullptr}};
        if (popup) for (const auto &key : {"visible", "opened", "opacity", "modal", "dim"})
            state[key] = QJsonValue::fromVariant(popup->property(key));
        popups[name] = state;
    }
    return {{"elapsed_ms", m_elapsed.elapsed()}, {"stage", m_stage}, {"selected_account", m_backend.account()},
        {"generation", m_backend.m_generation}, {"connected", m_backend.connected()}, {"shutdown", m_backend.m_shutdown},
        {"draft_dirty", m_backend.m_draftDirty}, {"draft_busy", m_backend.draftBusy()}, {"saving_draft", m_backend.m_savingDraft},
        {"resolving_draft", m_backend.m_resolvingDraft}, {"recipient_input_count", m_backend.m_recipientInputs.size()},
        {"unsaved_buffer", m_backend.unsavedAccountBuffer()}, {"loading", m_backend.loading()},
        {"auth_phase", m_backend.googleAuth().value("phase").toString()}, {"auth_busy", m_backend.googleAuthBusy()},
        {"target_lifecycle_state", life.value("state").toString()}, {"target_lifecycle_revision", life.value("revision").toLongLong()},
        {"removal_busy", m_backend.m_accountRemovalBusy.contains(id)}, {"removal_uncertain", ui.value("uncertain").toBool()},
        {"confirmation_account", m_confirmation["account"]}, {"confirmation_generation", m_confirmation["ui_generation"]},
        {"backend_confirmation_account", m_backend.m_accountRemovalConfirmation.value("account")},
        {"backend_confirmation_generation", m_backend.m_accountRemovalConfirmation.value("ui_generation")},
        {"pending_previews", pending}, {"popups", popups}, {"settings_open", m_window && m_window->property("settingsOpen").toBool()}};
}
void GuiAccountLifecycleSelfTest::observedControlClick() {
    if (m_clickedSignals.size() >= 16) { ++m_droppedClickedSignals; return; }
    m_clickedSignals.append(QJsonObject{{"control", sender() ? sender()->objectName() : QString{}}, {"state", diagnosticState()}});
}
void GuiAccountLifecycleSelfTest::acceptBrowserUrl(const QUrl &url) {
    if (!m_probe || url.scheme() != "https" || url.host() != "accounts.google.com" || url.path() != "/o/oauth2/v2/auth") {
        finish("Unexpected browser handoff in isolated lifecycle check."); return;
    }
    const QUrlQuery query(url);
    if (m_account < 0 || m_account >= m_expected.size()
        || query.allQueryItemValues("login_hint").size() != 1
        || query.queryItemValue("login_hint", QUrl::FullyDecoded) != m_expected[m_account].toObject()["email"].toString()
        || named(m_window->contentItem(), "googleClientConfigPath")) {
        finish("Native sign-in did not use the entered email and app registration."); return;
    }
    ++m_browserCount; // Never retain URL, state, code, token or authorization headers.
}
bool GuiAccountLifecycleSelfTest::click(const QString &name) {
    if (!m_window) return false;
    auto item = named(m_window->contentItem(), name);
    if (!item || !item->isVisible() || !item->isEnabled() || item->width() <= 0 || item->height() <= 0) return false;
    for (auto ancestor = item->parentItem(); ancestor; ancestor = ancestor->parentItem()) {
        if (!ancestor->property("contentY").isValid()) continue;
        const auto point = item->mapToItem(ancestor, {item->width()/2, item->height()/2});
        if (point.y() >= 8 && point.y() < ancestor->height()-8) continue;
        const auto origin = ancestor->property("originY").toDouble();
        const auto maximum = qMax(origin, origin + ancestor->property("contentHeight").toDouble() - ancestor->height());
        const auto next = qBound(origin, ancestor->property("contentY").toDouble() + point.y() - ancestor->height()/2, maximum);
        if (qAbs(next - ancestor->property("contentY").toDouble()) > .5) { ancestor->setProperty("contentY", next); return false; }
    }
    QPointer<QQuickItem> control = item;
    const auto point = item->mapToScene({item->width()/2, item->height()/2});
    if (!QRectF(0, 0, m_window->width(), m_window->height()).contains(point)) return false;
    const bool traced = name.startsWith("removeAccount_") || name == "confirmAccountRemoval";
    QJsonObject trace;
    if (traced) {
        QObject::connect(item, SIGNAL(clicked()), this, SLOT(observedControlClick()), Qt::UniqueConnection);
        QJsonArray hits;
        auto hit = m_window->contentItem();
        for (int depth = 0; hit && depth < 16; ++depth) {
            hits.append(QJsonObject{{"name", hit->objectName()}, {"type", hit->metaObject()->className()},
                {"visible", hit->isVisible()}, {"enabled", hit->isEnabled()}, {"opacity", hit->opacity()}});
            const auto local = hit->mapFromScene(point); hit = hit->childAt(local.x(), local.y());
        }
        const auto rect = item->mapRectToScene(item->boundingRect());
        trace = {{"control", name}, {"before", diagnosticState()}, {"center_x", point.x()}, {"center_y", point.y()},
            {"scene_x", rect.x()}, {"scene_y", rect.y()}, {"width", rect.width()}, {"height", rect.height()},
            {"visible", item->isVisible()}, {"enabled", item->isEnabled()}, {"visual_hit_chain", hits}};
    }
    const QPointF global = m_window->mapToGlobal(point.toPoint());
    QMouseEvent move(QEvent::MouseMove, point, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier); QCoreApplication::sendEvent(m_window, &move); if (!control) return false;
    QMouseEvent press(QEvent::MouseButtonPress, point, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier); QCoreApplication::sendEvent(m_window, &press);
    QMouseEvent release(QEvent::MouseButtonRelease, point, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier); QCoreApplication::sendEvent(m_window, &release);
    if (traced) {
        trace["move_accepted"] = move.isAccepted(); trace["press_accepted"] = press.isAccepted(); trace["release_accepted"] = release.isAccepted();
        trace["control_survived"] = bool(control); trace["after"] = diagnosticState();
        if (m_dispatches.size() < 16) m_dispatches.append(trace); else ++m_droppedDispatches;
    }
    return true;
}
bool GuiAccountLifecycleSelfTest::type(const QString &name, const QString &text, bool accept) {
    auto item = named(m_window->contentItem(), name); if (!item || !item->isVisible() || !item->isEnabled() || item->property("readOnly").toBool()) return false;
    item->forceActiveFocus(); QMetaObject::invokeMethod(item, "selectAll");
    QInputMethodEvent input; input.setCommitString(text); QCoreApplication::sendEvent(item, &input);
    auto document = qvariant_cast<QQuickTextDocument *>(item->property("textDocument"));
    if ((document ? document->textDocument()->toPlainText() : item->property("text").toString()) != text) return false;
    if (accept) { QKeyEvent press(QEvent::KeyPress, Qt::Key_Return, Qt::NoModifier), release(QEvent::KeyRelease, Qt::Key_Return, Qt::NoModifier); QCoreApplication::sendEvent(item, &press); QCoreApplication::sendEvent(item, &release); }
    return true;
}
bool GuiAccountLifecycleSelfTest::step(const QString &identity, bool renamed) {
    if (!m_probe || (identity != "personal" && identity != "workspace") || m_step >= 64) { finish("Unadmitted fixture step."); return false; }
    QSaveFile file(QDir(m_backend.m_profile).filePath("verification-google-account-step.json"));
    const auto bytes = QJsonDocument(QJsonObject{{"step", ++m_step}, {"identity", identity}, {"outcome", "success"}, {"grant", "full"}, {"renamed", renamed}}).toJson(QJsonDocument::Compact);
    if (!file.open(QIODevice::WriteOnly) || file.write(bytes) != bytes.size() || !file.commit()) { finish("Cannot retain the isolated fixture step."); return false; } return true;
}
bool GuiAccountLifecycleSelfTest::marker(const QString &name, bool present) {
    if (!m_probe || name != "verification-google-cleanup-fail") { finish("Unadmitted fixture marker."); return false; }
    QFile file(QDir(m_backend.m_profile).filePath(name));
    if (!present) return !file.exists() || file.remove();
    return file.open(QIODevice::WriteOnly | QIODevice::NewOnly);
}
bool GuiAccountLifecycleSelfTest::captureProviderBeforeRestore() {
    if (!m_probe) { finish("Provider baseline requires the isolated account handshake."); return false; }
    if (!m_providerBeforeRestore.isEmpty()) return true;
    for (const auto &expected : m_expected) {
        const auto state = m_backend.googleSyncState(expected.toObject()["account"].toString());
        if (state.value("owned").toBool() || state.value("active").toBool() || state.value("changing").toBool()) {
            finish("Provider baseline requires both explicit sync sessions to be paused."); return false;
        }
    }
    if (!m_beforeRestoreCutoff) m_beforeRestoreCutoff = QDateTime::currentMSecsSinceEpoch();
    QFile report(QDir(m_backend.m_profile).filePath("verification-google-report.json"));
    if (!report.open(QIODevice::ReadOnly) || report.size() > 256 * 1024) { finish("Isolated provider baseline is missing or oversized."); return false; }
    const auto fixture = QJsonDocument::fromJson(report.readAll()).object();
    const auto updated = fixture["updated_unix_ms"].toInteger();
    if (updated < m_beforeRestoreCutoff) return false; // Existing tick observes the next ordinary report.
    const auto counters = fixture["counters"].toObject();
    if (fixture["transport"] != "in_memory_only" || fixture["native_vault"] != false
        || fixture["account_fixture"].toObject()["mode"] != "accounts-v1" || counters.isEmpty() || counters.size() > 64) {
        finish("Unexpected isolated provider baseline."); return false;
    }
    for (const auto &counter : counters) if (!counter.isDouble() || counter.toInteger(-1) < 0) {
        finish("Invalid isolated provider counter."); return false;
    }
    m_providerBeforeRestore = {{"captured_after_unix_ms", m_beforeRestoreCutoff}, {"updated_unix_ms", updated}, {"counters", counters}};
    return true;
}
bool GuiAccountLifecycleSelfTest::clickDraft(const QString &id) {
    if (m_backend.loading() || m_backend.folder() != "drafts") return false;
    auto list = named(m_window->contentItem(), "conversationList"); if (!list) return false;
    for (int index = 0; index < m_backend.conversations().size(); ++index) if (m_backend.conversations()[index].toMap().value("draft_id").toString() == id) {
        if (!named(m_window->contentItem(), "conversationRow_" + id)) { QMetaObject::invokeMethod(list, "positionViewAtIndex", Q_ARG(int, index), Q_ARG(int, 3)); return false; }
        return click("conversationRow_" + id);
    }
    return false;
}
QJsonObject GuiAccountLifecycleSelfTest::cli(const QString &account, const QStringList &arguments) {
    QProcess process;
    process.start(m_cli, QStringList{"--profile", m_backend.m_profile, "--engine", m_backend.m_engine, "--account", account, "--json"} + arguments);
    // Only this GUI event dispatch is paused; the independent CLI/engine makes
    // the real removal between authored input and the ordinary autosave timer.
    if (!process.waitForStarted(5000) || !process.waitForFinished(10000)) { process.kill(); process.waitForFinished(1000); finish("Independent account CLI timed out."); return {}; }
    const auto response = QJsonDocument::fromJson(process.readAllStandardOutput()).object();
    if (process.exitStatus() != QProcess::NormalExit || process.exitCode() != 0 || !response["ok"].toBool()) { finish("Independent account lifecycle CLI refused its generated request."); return {}; }
    return response["result"].toObject();
}
void GuiAccountLifecycleSelfTest::tick() {
    if (m_finished || !m_window) return;
    if (m_loggedStage != m_stage) {
        m_loggedStage = m_stage; qInfo() << "Native account lifecycle stage" << m_stage << "account" << m_account << "elapsed_ms" << m_elapsed.elapsed();
        if (m_stages.size() < 80) m_stages.append(diagnosticState()); else ++m_droppedStages;
    }
    const auto expected = m_expected[m_account].toObject(), authored = expected["draft"].toObject();
    const auto id = expected["account"].toString();
    auto row = QJsonObject::fromVariantMap(m_backend.lifecycleAccount(id));
    const auto cleanup = row["lifecycle"].toObject()["cleanup"].toObject()["state"].toString();
    auto result = m_results.size() > m_account ? m_results[m_account].toObject() : QJsonObject{{"account", id}};
    const auto savedId = result["saved_draft"].toObject()["draft_id"].toString();
    const auto recordRemoval = [&] { auto operations = result["removals"].toArray(); operations.append(QJsonObject::fromVariantMap(m_backend.m_accountLifecycleUi.value(id).toMap().value("result").toMap())); result["removals"] = operations; m_results[m_account] = result; };
    switch (m_stage) {
    case 0:
        if (const auto loader = named(m_window->contentItem(), "startupOverlay"); loader && loader->isVisible()) return;
        if (!m_backend.connected()) return;
        if (m_backend.syntheticProfile()) { finish("Lifecycle verification requires the isolated live-mode fixture."); return; }
        m_stage = 1;
        m_backend.request("verification_google_auth_fixture", {}, {}, [this](auto response, bool ok) {
            if (!ok || response != QJsonObject{{"mode", "accounts-v1"}, {"browser_stub_allowed", true}}) { finish("Isolated account browser handshake refused."); return; }
            m_probe = true; QDesktopServices::setUrlHandler("https", this, "acceptBrowserUrl");
            if (!m_backend.allAccounts().isEmpty()) { finish("Native account scenario must begin with zero bound accounts."); return; }
            m_stage = 2;
        }); return;
    case 1: return;
    case 2: if (click("settingsButton")) m_stage = 3; break;
    case 3: if (click("settingsPage_Accounts") && step(expected["identity"].toString())) m_stage = 4; break;
    case 4: if (click("connectGoogleButton")) m_stage = 5; break;
    case 5: if (type("googleEmailAddress", expected["email"].toString()) && click("beginGoogleSignIn")) m_stage = 6; break;
    case 6:
        if (m_backend.googleAuthBusy() || m_backend.googleAuth().value("phase") != "connected" || row.isEmpty()) return;
        if (row["email"] != expected["email"] || row["lifecycle"].toObject()["state"] != "active") { finish("Native Add bound a different account."); return; }
        result["add_lifecycle"] = row["lifecycle"]; m_results.append(result); m_backend.setCalendarDate(m_date); m_stage = 7; break;
    case 7: if (click("googleSync_" + id)) m_stage = 8; break;
    case 8: {
        if (m_window->findChild<QObject *>("googleSyncLimitsDialog")) { finish("Removed quota setup is still present."); return; }
        if (m_backend.googleSyncState(id).value("owned").toBool()) {
            if (QJsonObject::fromVariantMap(m_backend.googleSyncLimits()) != m_limits) { finish("Built-in sync limits differ from independent expectation."); return; }
            m_stage = 9;
        }
        break;
    }
    case 9: {
        QJsonArray events; for (const auto &value : m_backend.events()) { const auto event = value.toMap(); if (event.value("account").toString() == id && event.value("event_id").toString() == "ci0000") events.append(event.value("event_id").toString()); }
        if (row["message_count"].toInt() < 2 || events.size() != 1) return;
        result["acquired"] = QJsonObject{{"message_count", row["message_count"]}, {"event_ids", events}}; m_results[m_account] = result;
        if (click("googleSync_" + id)) m_stage = 10; break;
    }
    case 10: if (!m_backend.googleSyncState(id).value("owned").toBool() && !m_backend.googleSyncState(id).value("changing").toBool() && click("settingsDoneButton")) m_stage = 11; break;
    case 11: if (click("accountTab_" + id)) m_stage = 12; break;
    case 12: if (m_backend.account() == id && click("newMessageButton")) m_stage = 13; break;
    case 13:
        if (m_backend.draftBusy() || m_backend.draft().value("mode") != "new") return;
        if (type("draftSubject", authored["subject"].toString()) && type("draftRecipient_to_recipients", authored["recipient"].toString(), true) && type("replyComposer", authored["body"].toString())) m_stage = 14;
        break;
    case 14:
        if (m_backend.m_draftDirty || m_backend.draftBusy() || m_backend.draft().value("revision").toLongLong() <= 0) return;
        result["saved_draft"] = QJsonObject::fromVariantMap(m_backend.draft()); m_results[m_account] = result;
        if (m_account == 0) { m_account = 1; m_stage = 2; } else { m_account = 0; m_stage = 20; } break;
    case 20: if (click("accountTab_" + id)) m_stage = 21; break;
    case 21: if (m_backend.account() == id && click("draftsScopeButton")) m_stage = 22; break;
    case 22: if (clickDraft(savedId)) m_stage = 23; break;
    case 23:
        if (m_backend.activeDraftId() != savedId || m_backend.draftBusy()) return;
        if (type("replyComposer", authored["dirty_body"].toString()) && click("settingsButton")) m_stage = 24; break;
    case 24: if (click("settingsPage_Accounts") && marker("verification-google-cleanup-fail", true)) m_stage = 25; break;
    case 25: if (click("removeAccount_" + id)) m_stage = 26; break;
    case 26:
        if (m_confirmation["account"] != id || m_backend.m_draftDirty || m_backend.draftBusy()) return;
        result["saved_before_remove"] = QJsonObject::fromVariantMap(m_backend.draft()); result["remove_confirmation"] = m_confirmation; m_results[m_account] = result;
        if (click("confirmAccountRemoval")) m_stage = 27; break;
    case 27:
        if (!m_backend.accountRemoved(id) || cleanup != "failed" || m_backend.m_accountRemovalBusy.contains(id)) return;
        recordRemoval(); if (!marker("verification-google-cleanup-fail", false)) { finish("Cannot release owned cleanup failure marker."); return; } m_stage = 28; break;
    case 28: if (click("retryAccountCleanup_" + id)) m_stage = 29; break;
    case 29: if (cleanup == "complete" && !m_backend.m_accountRemovalBusy.contains(id)) { recordRemoval(); m_stage = 30; } break;
    case 30: if (click("openAccountArchive_" + id)) m_stage = 31; break;
    case 31: if (m_backend.account() == id && m_backend.accountArchived() && click("draftsScopeButton")) m_stage = 32; break;
    case 32: if (clickDraft(savedId)) m_stage = 33; break;
    case 33: {
        if (m_backend.activeDraftId() != savedId || m_backend.draftBusy()) return;
        const auto draft = QJsonObject::fromVariantMap(m_backend.draft());
        if (!m_backend.accountArchived() || !m_backend.draftLocked() || draft["body"] != authored["dirty_body"] || draft["from"] != expected["email"]) { finish("Archived draft differs from the version saved before removal."); return; }
        const auto frozen = m_backend.draft(); m_backend.editDraft("must not save"); m_backend.discardDraft(); m_backend.captureDraft(); m_backend.sendGoogleDraft();
        if (m_backend.draft() != frozen || m_backend.m_draftDirty || m_backend.m_draftTimer.isActive()) { finish("Archived controls admitted authored work."); return; }
        for (const auto &value : m_backend.events()) if (value.toMap().value("account").toString() == id) return;
        result["archive"] = QJsonObject{{"draft", draft}, {"combined_calendar_excluded", true}}; m_results[m_account] = result;
        if (click("inboxScopeButton")) m_stage = 34; break;
    }
    case 34:
        if (m_backend.loading() || m_backend.conversations().isEmpty()) return;
        m_conversation = m_backend.conversations().first().toMap().value("id").toString(); m_stage = 35;
        m_backend.request("conversation", id, {{"id", m_conversation}}, [this](auto value, bool ok) { if (!ok) { finish("Archived cached source read failed."); return; } m_unreadBefore = unread(value); m_stage = 36; }); break;
    case 35: return;
    case 36: if (click("conversationRow_" + m_conversation)) m_stage = 37; break;
    case 37:
        if (m_backend.conversation().value("id").toString() != m_conversation || m_backend.conversation().value("messages").toList().isEmpty()) return;
        if (m_unreadBefore.size() != 2 || unread(QJsonObject::fromVariantMap(m_backend.conversation())) != m_unreadBefore) { finish("Archived deliberate open changed unread state."); return; }
        for (const auto &name : {"replyButton", "forwardButton"}) { auto control = named(m_window->contentItem(), name); if (!control || control->isEnabled()) { finish("Archived reader exposed an enabled authoring control."); return; } }
        { auto archive = result["archive"].toObject(); archive["conversation_id"] = m_conversation; archive["unread_before"] = m_unreadBefore; archive["unread_after"] = unread(QJsonObject::fromVariantMap(m_backend.conversation())); result["archive"] = archive; m_results[m_account] = result; }
        if (click("settingsButton")) m_stage = 38; break;
    case 38: if (captureProviderBeforeRestore() && click("settingsPage_Accounts") && step("personal", true)) m_stage = 39; break;
    case 39: if (click("restoreAccount_" + id)) m_stage = 40; break;
    case 40: if (type("googleEmailAddress", expected["email"].toString()) && click("beginGoogleSignIn")) m_stage = 41; break;
    case 41:
        if (m_backend.googleAuthBusy() || row["lifecycle"].toObject()["state"] != "active" || row["email"] != expected["renamed_email"]) return;
        if (m_backend.googleSyncState(id).value("owned").toBool() || m_backend.googleSyncState(id).value("active").toBool()) { finish("Restore started provider work."); return; }
        result["restore_lifecycle"] = row["lifecycle"]; result["restored_email"] = row["email"]; m_results[m_account] = result; m_confirmation = {}; m_stage = 42; break;
    case 42: if (click("removeAccount_" + id)) m_stage = 43; break;
    case 43: if (m_confirmation["account"] == id && click("confirmAccountRemoval")) m_stage = 44; break;
    case 44: if (cleanup == "complete" && m_backend.accountRemoved(id) && !m_backend.m_accountRemovalBusy.contains(id)) { recordRemoval(); m_account = 1; m_stage = 50; } break;
    case 50: if (click("accountTab_" + id)) m_stage = 51; break;
    case 51: if (m_backend.account() == id && click("draftsScopeButton")) m_stage = 52; break;
    case 52: if (clickDraft(savedId)) m_stage = 53; break;
    case 53: if (m_backend.activeDraftId() == savedId && !m_backend.draftBusy() && click("draftShowBcc")) m_stage = 54; break;
    case 54: {
        if (!type("replyComposer", authored["dirty_body"].toString()) || !type("draftRecipient_bcc_recipients", authored["pending_recipient"].toString())) return;
        m_buffer = {{"account", id}, {"draft", QJsonObject::fromVariantMap(m_backend.draft())}, {"recipient_inputs", QJsonObject::fromVariantMap(m_backend.recipientInputs())}};
        const auto preview = cli(id, {"account", "remove-preview"}); if (m_finished) return;
        const auto requestId = "gui-external-remove-" + QUuid::createUuid().toString(QUuid::WithoutBraces);
        m_external = cli(id, {"account", "remove", "--expected-revision", QString::number(preview["lifecycle"].toObject()["revision"].toInteger()), "--request-id", requestId, "--retention", "archive"});
        if (m_finished) return; m_backend.loadAccounts(); m_stage = 55; break;
    }
    case 55:
        if (!m_backend.unsavedAccountBuffer() || !m_backend.accounts().isEmpty()) return;
        if (m_backend.account() != id || QJsonObject::fromVariantMap(m_backend.draft()) != m_buffer["draft"].toObject()
            || QJsonObject::fromVariantMap(m_backend.recipientInputs()) != m_buffer["recipient_inputs"].toObject()) { finish("CLI removal changed or retargeted the unsaved editor."); return; }
        m_backend.clearError(); if (click("settingsButton")) m_stage = 56; break;
    case 56: if (click("keepRemovedAccountBuffer")) { if (!m_backend.unsavedAccountBuffer() || m_window->property("settingsOpen").toBool()) { finish("Keep here lost the buffer or allowed navigation."); return; } m_disposition["keep_here_preserved"] = true; m_disposition["navigation_guarded"] = true; m_window->close(); m_stage = 57; } break;
    case 57: if (click("keepRemovedAccountBuffer")) { if (m_backend.m_shutdown || !m_window->isVisible()) { finish("Close bypassed unsaved disposition."); return; } m_disposition["close_guarded"] = true; m_stage = 58; } break;
    case 58: if (click("collapseReply")) m_stage = 59; break;
    case 59: if (click("discardRemovedAccountBuffer")) m_stage = 60; break;
    case 60:
        if (!m_backend.account().isEmpty() || m_backend.unsavedAccountBuffer() || !m_backend.conversations().isEmpty()) { finish("Last-account buffer disposition did not clear active selection."); return; }
        m_disposition["explicit_discard_completed"] = true;
        if (click("settingsButton")) m_stage = 61; break;
    case 61: if (click("settingsPage_Accounts")) m_stage = 62; break;
    case 62: if (click("openAccountArchive_" + id)) m_stage = 63; break;
    case 63: if (m_backend.account() == id && click("draftsScopeButton")) m_stage = 64; break;
    case 64: if (clickDraft(savedId)) m_stage = 65; break;
    case 65:
        if (m_backend.activeDraftId() != savedId || m_backend.draftBusy()) return;
        if (m_backend.draft().value("body").toString() != authored["body"].toString() || !m_backend.accountArchived()) { finish("Discarding the unsaved buffer changed the saved archive."); return; }
        result["archive"] = QJsonObject{{"draft", QJsonObject::fromVariantMap(m_backend.draft())}}; result["removals"] = QJsonArray{m_external}; m_results[m_account] = result;
        if (click("inboxScopeButton")) m_stage = 66; break;
    case 66:
        if (m_backend.loading() || m_backend.conversations().isEmpty()) return;
        m_conversation = m_backend.conversations().first().toMap().value("id").toString(); m_stage = 67;
        m_backend.request("conversation", id, {{"id", m_conversation}}, [this](auto value, bool ok) { if (!ok) { finish("Last archive cached source read failed."); return; } m_unreadBefore = unread(value); m_stage = 68; }); break;
    case 67: return;
    case 68: if (click("conversationRow_" + m_conversation)) m_stage = 69; break;
    case 69:
        if (m_backend.conversation().value("id").toString() != m_conversation || m_backend.conversation().value("messages").toList().isEmpty()) return;
        if (m_unreadBefore.size() != 2 || unread(QJsonObject::fromVariantMap(m_backend.conversation())) != m_unreadBefore) { finish("Last archived open changed unread state."); return; }
        { auto archive = result["archive"].toObject(); archive["conversation_id"] = m_conversation; archive["unread_before"] = m_unreadBefore; archive["unread_after"] = unread(QJsonObject::fromVariantMap(m_backend.conversation())); result["archive"] = archive; m_results[m_account] = result; }
        finish(); return;
    }
}
void GuiAccountLifecycleSelfTest::finish(const QString &error) {
    if (m_finished) return;
    m_finished = true; m_timer.stop(); m_watchdog.stop(); if (m_probe) { QDesktopServices::unsetUrlHandler("https"); marker("verification-google-cleanup-fail", false); }
    QString failureImage;
    if (!error.isEmpty() && m_probe && m_window) {
        failureImage = m_report.endsWith(".json") ? m_report.left(m_report.size() - 5) + ".failure.png" : m_report + ".failure.png";
        if (QFileInfo::exists(failureImage) || !m_window->grabWindow().save(failureImage)) failureImage.clear();
    }
    const QJsonObject result{{"version", 1}, {"result", error.isEmpty() ? "passed" : "failed"}, {"error", error}, {"stage", m_stage},
        {"gui_pid", QCoreApplication::applicationPid()}, {"accounts", m_results}, {"browser_handoffs", m_browserCount}, {"fixture_step_count", m_step},
        {"active_account_count", m_backend.accounts().size()}, {"archive_account_count", m_backend.allAccounts().size() - m_backend.accounts().size()},
        {"external_removal", m_external}, {"retained_buffer", m_buffer}, {"disposition", m_disposition}, {"unsaved_buffer", m_backend.unsavedAccountBuffer()},
        {"provider_before_restore", m_providerBeforeRestore},
        {"diagnostic", QJsonObject{{"final", diagnosticState()}, {"stages", m_stages}, {"dispatches", m_dispatches}, {"clicked_signals", m_clickedSignals},
            {"dropped_stages", m_droppedStages}, {"dropped_dispatches", m_droppedDispatches}, {"dropped_clicked_signals", m_droppedClickedSignals}, {"failure_png", failureImage}}}};
    QSaveFile file(m_report);
    if (QFileInfo::exists(m_report) || !file.open(QIODevice::WriteOnly) || file.write(QJsonDocument(result).toJson()) < 0 || !file.commit()) { QCoreApplication::exit(5); return; }
    m_passed = error.isEmpty();
    if (!m_passed) { qCritical().noquote() << error; QCoreApplication::exit(5); return; }
    qInfo() << "Native account addition, removal, archive, restoration and dirty-buffer lifecycle passed.";
    m_backend.setProperty("selfTestCompleted", true); m_backend.shutdown();
}
