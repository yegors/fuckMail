#include "GuiGoogleDraftSnapshotSelfTest.h"
#include "MailBackend.h"
#include "ReplyDocument.h"
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QFile>
#include <QFileInfo>
#include <QGuiApplication>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QQuickItem>
#include <QQuickWindow>
#include <QSaveFile>

namespace {
QQuickItem *itemNamed(QQuickItem *root, const QString &name) {
    if (!root) return nullptr;
    if (root->objectName() == name) return root;
    for (auto child : root->childItems()) if (auto found = itemNamed(child, name)) return found;
    return nullptr;
}
bool matches(const QJsonValue &actual, const QJsonValue &expected) {
    if (!expected.isObject()) return actual == expected;
    if (!actual.isObject()) return false;
    const auto object = actual.toObject(), subset = expected.toObject();
    for (auto it = subset.begin(); it != subset.end(); ++it)
        if (!object.contains(it.key()) || !matches(object[it.key()], it.value())) return false;
    return true;
}
QString fileHash(const QString &path) {
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly) || file.size() > 35 * 1024 * 1024) return {};
    QCryptographicHash hash(QCryptographicHash::Sha256);
    if (!hash.addData(&file)) return {};
    return QString::fromLatin1(hash.result().toHex());
}
}

GuiGoogleDraftSnapshotSelfTest::GuiGoogleDraftSnapshotSelfTest(MailBackend &backend, QQuickWindow *window,
        const QString &expectation, const QString &report, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_report(report) {
    if (QFileInfo::exists(report)) {
        m_finished = true;
        qCritical() << "Refusing to overwrite remote draft native report:" << report;
        QTimer::singleShot(0, this, [] { QCoreApplication::exit(5); }); return;
    }
    QFile input(expectation);
    if (!input.open(QIODevice::ReadOnly) || input.size() > 1024 * 1024) {
        QTimer::singleShot(0, this, [this] { finish("Missing or oversized remote draft expectation."); }); return;
    }
    const auto expected = QJsonDocument::fromJson(input.readAll()).object();
    m_accounts = expected["accounts"].toArray();
    if (expected["version"] != 1 || m_accounts.isEmpty() || m_accounts.size() > 4 || !window) {
        QTimer::singleShot(0, this, [this] { finish("Invalid remote draft expectation or native window."); }); return;
    }
    for (const auto &value : m_accounts) {
        const auto row = value.toObject();
        if (row["account"].toString().isEmpty() || row["provider_draft_id"].toString().isEmpty()
            || row["draft"].toObject().isEmpty() || row["raw_sha256"].toString().size() != 64) {
            QTimer::singleShot(0, this, [this] { finish("An expected account lacks exact draft/source identity."); }); return;
        }
    }
    window->setWidth(1280); window->setHeight(900);
    window->setProperty("workspace", "mail"); window->setProperty("settingsOpen", false);
    m_timer.setInterval(50);
    connect(&m_timer, &QTimer::timeout, this, &GuiGoogleDraftSnapshotSelfTest::tick);
    m_watchdog.setSingleShot(true);
    connect(&m_watchdog, &QTimer::timeout, this, [this] {
        finish(QString("Remote draft walkthrough timed out at stage %1, visit %2, account %3, draft %4.")
            .arg(m_stage).arg(m_visit).arg(m_backend.account(), m_backend.activeDraftId()));
    });
    m_watchdog.start(90000); m_timer.start();
}

bool GuiGoogleDraftSnapshotSelfTest::click(const QString &name) {
    if (!m_window) return false;
    auto item = itemNamed(m_window->contentItem(), name);
    if (!item || !item->isVisible() || !item->isEnabled() || item->width() <= 0 || item->height() <= 0) return false;
    // Reveal through normal Flickable geometry, then allow its layout to settle
    // on the next tick before delivering real window mouse events.
    for (auto ancestor = item->parentItem(); ancestor; ancestor = ancestor->parentItem()) {
        if (!ancestor->property("contentY").isValid()) continue;
        const auto centre = item->mapToItem(ancestor, {item->width()/2, item->height()/2});
        if (centre.y() >= 8 && centre.y() < ancestor->height()-8) continue;
        const double origin = ancestor->property("originY").toDouble();
        const double max = qMax(origin, origin + ancestor->property("contentHeight").toDouble() - ancestor->height());
        const double next = qBound(origin, ancestor->property("contentY").toDouble() + centre.y() - ancestor->height()/2, max);
        if (qAbs(next - ancestor->property("contentY").toDouble()) > .5) {
            ancestor->setProperty("contentY", next); return false;
        }
    }
    QPointer<QQuickItem> control = item;
    const auto p = item->mapToScene({item->width()/2, item->height()/2});
    if (!QRectF(0, 0, m_window->width(), m_window->height()).contains(p)) return false;
    const QPointF global = m_window->mapToGlobal(p.toPoint());
    QMouseEvent move(QEvent::MouseMove, p, global, Qt::NoButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &move);
    if (!control) return false;
    QMouseEvent press(QEvent::MouseButtonPress, p, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &press);
    QMouseEvent release(QEvent::MouseButtonRelease, p, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &release);
    return true;
}

bool GuiGoogleDraftSnapshotSelfTest::verifyReadOnlyControls(MailBackend &backend, QQuickItem *root, QString &error) {
    auto body = itemNamed(root, "replyComposer");
    auto subject = itemNamed(root, "draftSubject");
    auto document = root->window() ? root->window()->findChild<ReplyDocument *>("replyDocument") : root->findChild<ReplyDocument *>("replyDocument");
    if (!backend.draftReadOnly() || backend.m_draftDirty || !body || !subject || !document
        || !body->property("readOnly").toBool() || !subject->property("readOnly").toBool() || !document->readOnly()
        || !body->isEnabled() || !body->property("selectByMouse").toBool()) {
        error = QString("Imported editor boundary: provenance=%1 dirty=%2 body=%3 subject=%4 document=%5 bodyReadOnly=%6 subjectReadOnly=%7 documentReadOnly=%8 enabled=%9 selectable=%10.")
            .arg(backend.draftReadOnly()).arg(backend.m_draftDirty).arg(bool(body)).arg(bool(subject)).arg(bool(document))
            .arg(body && body->property("readOnly").toBool()).arg(subject && subject->property("readOnly").toBool())
            .arg(document && document->readOnly()).arg(body && body->isEnabled()).arg(body && body->property("selectByMouse").toBool());
        return false;
    }
    const auto before = backend.m_draft;
    const auto plain = document->plainText(), html = document->html();
    const auto pending = backend.m_pending.size();
    QMetaObject::invokeMethod(body, "selectAll");
    document->format("bold"); document->format("italic"); document->format("underline");
    document->pastePlain(); document->undo(); document->redo();
    QKeyEvent erase(QEvent::KeyPress, Qt::Key_Backspace, Qt::NoModifier);
    QCoreApplication::sendEvent(body, &erase);
    QKeyEvent type(QEvent::KeyPress, Qt::Key_X, Qt::NoModifier, "X");
    QCoreApplication::sendEvent(body, &type);
    QKeyEvent paste(QEvent::KeyPress, Qt::Key_V, Qt::ControlModifier);
    QCoreApplication::sendEvent(body, &paste);
    backend.editDraft("Must not replace remote text");
    backend.editFormattedDraft("Must not replace remote text", "<b>Must not format</b>");
    backend.setDraftSubject("Must not replace remote subject");
    backend.stageDraftRecipient("to_recipients", "other@example.test");
    const bool recipientAdded = backend.addDraftRecipient("cc_recipients", "other@example.test");
    backend.removeDraftRecipient("to_recipients", 0);
    backend.setDraftSignature("Must not append account signature");
    backend.setDraftAttachment("unrelated-local-file", true);
    backend.importDraftAttachment(QUrl::fromLocalFile("/never-read-native-source-test"), backend.account(), backend.activeDraftId());
    backend.retryDraftAttachment(); backend.saveMyDraftVersion(); backend.discardDraft(); backend.captureDraft();
    backend.reconcileGoogleDraft(); backend.replaceGoogleDraft({}); backend.sendGoogleDraft();
    backend.resumeGoogleSend(); backend.abandonGoogleSend(); backend.saveDraft();
    if (recipientAdded || backend.m_draft != before || backend.m_draftDirty || backend.m_draftTimer.isActive()
        || !backend.m_recipientInputs.isEmpty() || !backend.m_attachmentImports.isEmpty()
        || backend.m_pending.size() != pending || !backend.m_googleOperations.isEmpty()
        || document->plainText() != plain || document->html() != html) {
        error = "A mutation path changed the imported snapshot or dispatched work."; return false;
    }
    for (const auto &name : {"captureReply", "draftAttachButton", "discardReply"}) {
        auto control = itemNamed(root, name);
        if (control && control->isVisible()) { error = QString("Imported preview exposes authoring control %1.").arg(name); return false; }
    }
    return true;
}

bool GuiGoogleDraftSnapshotSelfTest::verifyProjection(QString &error) const {
    const auto expected = m_accounts[m_visit % m_accounts.size()].toObject();
    const auto draft = QJsonObject::fromVariantMap(m_backend.draft());
    const auto source = draft["remote_source"].toObject();
    if (m_backend.account() != expected["account"] || draft["account"] != expected["account"]
        || draft["origin"] != "google_snapshot" || draft["read_only"] != true
        || !draft["google"].isNull() || !draft["google_send"].isNull()
        || source["provider_draft_id"] != expected["provider_draft_id"]
        || source["sha256"] != expected["raw_sha256"] || !matches(draft, expected["draft"])) {
        error = "The cached native draft differs from the independently supplied account/source/routing/text expectation."; return false;
    }
    return true;
}
bool GuiGoogleDraftSnapshotSelfTest::verifyExport(const QString &attachmentId, const QString &hash, QString &error) {
    const auto copy = QJsonObject::fromVariantMap(m_backend.draftSources().value("export").toMap());
    const auto source = m_backend.draft().value("remote_source").toMap();
    if (copy["account"] != m_backend.account() || copy["draft_id"] != m_backend.activeDraftId()
        || copy["source_revision"].toInteger() != source.value("source_revision").toLongLong()
        || copy["attachment_id"].toString() != attachmentId || copy["sha256"] != hash
        || !QFileInfo(copy["path"].toString()).isAbsolute() || fileHash(copy["path"].toString()) != hash
        || QFileInfo(copy["path"].toString()).size() != copy["size"].toInteger(-1)) {
        error = "The exported source/file copy failed exact identity, byte hash or size verification."; return false;
    }
    m_exports.append(copy); return true;
}

void GuiGoogleDraftSnapshotSelfTest::tick() {
    if (m_finished || !m_window) return;
    if (!m_backend.error().isEmpty()) { finish(m_backend.error()); return; }
    const auto expected = m_accounts[m_visit % m_accounts.size()].toObject();
    const auto account = expected["account"].toString();
    QString error;
    switch (m_stage) {
    case 0:
        if (const auto loader = itemNamed(m_window->contentItem(), "startupOverlay"); loader && loader->isVisible()) return;
        if (!m_backend.connected() || m_backend.accounts().isEmpty()) return;
        if (m_backend.syntheticProfile()) { finish("This test requires the retained isolated live-mode fixture."); return; }
        m_stage = 1; break;
    case 1:
        if (click("accountTab_" + account)) m_stage = 2;
        break;
    case 2:
        if (m_backend.account() != account || m_backend.loading() || m_backend.draftBusy()) return;
        if (m_backend.googleSyncState(account).value("owned").toBool()) { finish("Offline native test must not own provider synchronization."); return; }
        if (!m_backend.query().isEmpty()) { m_backend.setQuery({}); return; }
        if (click("draftsScopeButton")) { m_pages = 0; m_stage = 3; }
        break;
    case 3: {
        if (m_backend.folder() != "drafts" || m_backend.loading()) return;
        int index = 0;
        m_draftId.clear();
        for (const auto &entry : m_backend.conversations()) {
            const auto row = entry.toMap();
            if (row.value("remote_source").toMap().value("provider_draft_id").toString() == expected["provider_draft_id"].toString()) {
                m_draftId = row.value("draft_id").toString(); break;
            }
            ++index;
        }
        auto list = itemNamed(m_window->contentItem(), "conversationList");
        if (!list) return;
        if (m_draftId.isEmpty()) {
            if (!m_backend.hasMore() || m_pages >= 10) { finish("Expected remote draft is missing from bounded native Drafts paging."); return; }
            QMetaObject::invokeMethod(list, "positionViewAtEnd");
            if (click("loadMoreConversations")) ++m_pages;
            return;
        }
        QMetaObject::invokeMethod(list, "positionViewAtIndex", Q_ARG(int, index), Q_ARG(int, 3));
        m_stage = 4; break;
    }
    case 4:
        if (click("conversationRow_" + m_draftId)) m_stage = 5;
        break;
    case 5:
        if (m_backend.activeDraftId() != m_draftId || m_backend.draftBusy()) return;
        if (!verifyProjection(error) || !verifyReadOnlyControls(m_backend, m_window->contentItem(), error)) { finish(error); return; }
        m_observed.append(QJsonObject{{"visit", m_visit}, {"draft", QJsonObject::fromVariantMap(m_backend.draft())},
            {"read_only_controls_verified", true}, {"provider_sync_owned", false}});
        if (m_visit >= m_accounts.size()) { finish(); return; }
        m_stage = 11; break;
    case 11:
        if (click("draftSourceHistoryButton")) m_stage = 6;
        break;
    case 6:
        if (m_backend.draftSources().value("loading").toBool()) return;
        if (click("exportDraftSource_" + QString::number(m_backend.draft().value("remote_source").toMap().value("source_revision").toLongLong()))) m_stage = 7;
        break;
    case 7:
        if (m_backend.draftSources().value("exporting").toBool()) return;
        if (!verifyExport({}, expected["raw_sha256"].toString(), error)) { finish(error); return; }
        m_attachment = 0; m_stage = 8; break;
    case 8:
        if (click("closeDraftSourceHistory")) m_stage = 9;
        break;
    case 9: {
        const auto attachments = expected["attachments"].toArray();
        if (m_attachment >= attachments.size()) { ++m_visit; m_stage = 1; break; }
        if (click("exportDraftAttachment_" + attachments[m_attachment].toObject()["id"].toString())) m_stage = 10;
        break;
    }
    case 10: {
        if (m_backend.draftSources().value("exporting").toBool()) return;
        const auto attachment = expected["attachments"].toArray()[m_attachment].toObject();
        if (!verifyExport(attachment["id"].toString(), attachment["sha256"].toString(), error)) { finish(error); return; }
        ++m_attachment; m_stage = 8; break;
    }
    }
}
void GuiGoogleDraftSnapshotSelfTest::finish(const QString &error) {
    if (m_finished) return;
    m_finished = true; m_timer.stop(); m_watchdog.stop();
    QJsonObject report{{"version", 1}, {"passed", error.isEmpty()}, {"error", error}, {"stage", m_stage},
        {"gui_pid", QCoreApplication::applicationPid()}, {"observed", m_observed}, {"exports", m_exports},
        {"scope", "Fresh native cached reopen; explicit local Watch may retain engine. No sync start or provider mutation."}};
    QSaveFile output(m_report);
    if (QFileInfo::exists(m_report) || !output.open(QIODevice::WriteOnly)
        || output.write(QJsonDocument(report).toJson()) < 0 || !output.commit()) {
        qCritical() << "Cannot retain native remote-draft report:" << m_report;
        QCoreApplication::exit(5); return;
    }
    m_passed = error.isEmpty();
    if (!m_passed) { qCritical().noquote() << "Native remote draft verification failed:" << error; QCoreApplication::exit(5); return; }
    qInfo() << "Native cached Google draft preview, mutation guards, source exports and account reopen passed.";
    m_backend.setProperty("selfTestCompleted", true); m_backend.shutdown();
}
