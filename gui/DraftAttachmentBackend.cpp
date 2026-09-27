// MailBackend's attachments: importing local files into a draft (queued, one at a
// time, with exact retry), and previewing, downloading or saving a message's attachments.
#include "MailBackend.h"
#include <QDir>
#include <QJsonArray>
#include <QUrl>
#include <QFile>
#include <QFileInfo>
#include <QSaveFile>
#include <QCryptographicHash>
#include <QStandardPaths>
#include <QImageReader>
#include <QVideoFrame>
#include <QVideoSink>
#include <QUuid>
#include <memory>

namespace {
QString importKey(const QString &account, const QString &draft) { return account + '\n' + draft; }
}

QString MailBackend::attachmentImportKey() const {
    return importKey(m_account, m_draft.value("draft_id").toString());
}
bool MailBackend::attachmentImportPending() const {
    const auto key = attachmentImportKey();
    return m_attachmentImports.contains(key) || !m_attachmentQueues.value(key).isEmpty();
}
QVariantMap MailBackend::attachmentImport() const {
    const auto key = attachmentImportKey();
    const auto queued = m_attachmentQueues.value(key).size();
    return {{"busy", m_attachmentImportActive == key}, {"pending", attachmentImportPending()},
        {"retry", m_attachmentImportRetry.contains(key)}, {"queued", queued},
        {"can_resume", queued > 0 && !m_attachmentImports.contains(key) && !attachmentImportBusy()},
        {"status", m_attachmentImportStatus.value(key) + (queued > 0 ? QString(" %1 file(s) waiting.").arg(queued) : QString{})}};
}

void MailBackend::importDraftAttachment(const QUrl &file, const QString &account, const QString &draftId) {
    importDraftAttachments({file}, account, draftId);
}

void MailBackend::importDraftAttachments(const QList<QUrl> &files, const QString &account, const QString &draftId) {
    // The native picker captures its origin before opening. A later selection
    // cannot attach to whichever draft happens to be displayed by then.
    if (account != m_account || draftId.isEmpty() || draftId != m_draft.value("draft_id").toString()) return;
    const auto key = importKey(account, draftId);
    auto reject = [this, key](const QString &message) { m_attachmentImportStatus[key] = message; emit draftChanged(); };
    if (!m_connected || m_shutdown || draftLocked()) { reject("Reconnect and finish the current draft operation before attaching a file."); return; }
    if (m_draftConflict) { reject("Resolve the saved draft conflict before attaching a file. Your edits remain here."); return; }
    if (attachmentImportPending()) { reject("Finish or skip the pending files before choosing more."); return; }
    if (files.isEmpty()) return;
    if (files.size() + m_draft.value("attachment_ids").toList().size() > 50) { reject("A draft supports at most 50 attachments."); return; }
    for (const auto &file : files)
        if (!file.isLocalFile() || !QDir::isAbsolutePath(file.toLocalFile())) { reject("Choose local files using absolute paths."); return; }
    QSet<QString> pending;
    for (auto it = m_attachmentImports.cbegin(); it != m_attachmentImports.cend(); ++it) pending.insert(it.key());
    for (auto it = m_attachmentQueues.cbegin(); it != m_attachmentQueues.cend(); ++it) pending.insert(it.key());
    if (pending.size() >= 8) { reject("Resolve the pending files in your other drafts first."); return; }
    QList<QUrl> unique;
    for (const auto &file : files) if (!unique.contains(file)) unique.append(file);
    m_attachmentQueues.insert(key, unique);
    m_attachmentQueueLifecycle[key] = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    m_attachmentQueuePaused.remove(key);
    m_attachmentImportStatus[key] = "Preparing attachments…";
    emit draftChanged();
    advanceDraftAttachments();
}

// Imports the next queued file. Files go one at a time because each import
// advances the draft revision that the next one must name.
void MailBackend::advanceDraftAttachments() {
    const auto key = attachmentImportKey();
    if (m_attachmentQueues.value(key).isEmpty() || m_attachmentQueuePaused.contains(key)
        || m_attachmentImports.contains(key) || attachmentImportBusy()
        || !m_connected || m_shutdown || draftLocked() || m_draftConflict) return;
    if (m_attachmentQueueLifecycle.value(key) != lifecycleAccount(m_account).value("lifecycle").toMap().value("revision").toLongLong()) {
        m_attachmentQueuePaused.insert(key);
        m_attachmentImportStatus[key] = "This account changed. Skip the waiting files and choose them again.";
        emit draftChanged(); return;
    }
    const bool needsSave = m_draft.value("revision").toLongLong() <= 0 || m_draft.value("discarded").toBool();
    if (m_draftDirty || draftBusy() || needsSave) {
        // A virtual or reopened discarded legacy reply first becomes a normal
        // saved draft, through the same revision/source checks as typed edits.
        if (needsSave) m_draftDirty = true;
        saveDraft(); return;
    }
    auto &queue = m_attachmentQueues[key];
    const auto file = queue.takeFirst();
    if (queue.isEmpty()) { m_attachmentQueues.remove(key); m_attachmentQueueLifecycle.remove(key); }
    auto params = mutation();
    params["account"] = m_account; params["draft_id"] = m_draft.value("draft_id").toString();
    params["expected_revision"] = m_draft.value("revision").toLongLong();
    params["path"] = file.toLocalFile();
    m_attachmentImports.insert(key, params);
    startDraftAttachmentImport(key);
}

void MailBackend::retryDraftAttachment() {
    if (draftLocked()) return;
    const auto key = attachmentImportKey();
    if (attachmentImportBusy() || draftBusy() || !m_connected || m_shutdown) return;
    m_attachmentQueuePaused.remove(key);
    if (!m_attachmentImportRetry.contains(key)) { advanceDraftAttachments(); return; }
    // Do not stat/reopen the path or regenerate the request/revision. Core
    // resolves a recorded receipt before touching a now-missing source file.
    startDraftAttachmentImport(key);
}

void MailBackend::skipQueuedDraftAttachments() {
    const auto key = attachmentImportKey();
    m_attachmentQueues.remove(key); m_attachmentQueuePaused.remove(key); m_attachmentQueueLifecycle.remove(key);
    if (!m_attachmentImports.contains(key)) m_attachmentImportStatus[key] = "Remaining files skipped.";
    emit draftChanged();
    saveDraft();
}

void MailBackend::startDraftAttachmentImport(const QString &key) {
    const auto intent = m_attachmentImports.value(key);
    if (intent.isEmpty() || attachmentImportBusy()) return;
    const auto account = intent["account"].toString();
    if (accountRemoved(account) || m_unsavedAccountBuffer) return;
    const auto lifecycle = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
    auto params = intent; params.remove("account");
    m_attachmentImportActive = key; m_attachmentImportRetry.remove(key);
    m_attachmentImportStatus[key] = "Attaching file…"; emit draftChanged();
    googleOperation("draft_attachment_import", account, params, [this, key, intent, lifecycle](auto result, bool ok) {
        m_attachmentImportActive.clear();
        const bool current = key == attachmentImportKey() && !accountArchived() && !m_unsavedAccountBuffer
            && lifecycle == lifecycleAccount(intent["account"].toString()).value("lifecycle").toMap().value("revision").toLongLong();
        const auto draft = result["draft"].toObject();
        const auto attachment = result["attachment"].toObject();
        if (ok && (draft["account"] != intent["account"] || draft["draft_id"] != intent["draft_id"]
            || attachment["account"] != intent["account"] || attachment["id"].toString().isEmpty()
            || !draft["attachment_ids"].toArray().contains(attachment["id"])
            || draft["revision"].toInteger() != intent["expected_revision"].toInteger() + 1)) {
            result = QJsonObject{{"code", "invalid_response"}, {"message", "Attachment result did not match the original draft. Retry to check its saved result."}};
            ok = false;
        }
        if (!ok) {
            m_attachmentQueuePaused.insert(key);
            // For transport-level failures the import may or may not have been
            // recorded; keep the exact request so Retry asks about the same one.
            const auto code = result["code"].toString();
            const bool uncertain = code == "timeout" || code == "engine_disconnected" || code == "cancelled"
                || code == "invalid_response" || code == "busy" || code == "engine_untrusted";
            if (uncertain) m_attachmentImportRetry.insert(key);
            else { m_attachmentImports.remove(key); m_attachmentImportRetry.remove(key); }
            m_attachmentImportStatus[key] = (uncertain ? "Attachment result not confirmed — " : "Not attached — ")
                + result["message"].toString("the import failed. Your edits remain here.");
            if (current && code == "stale_revision") {
                m_draftConflict = true; m_draftStatus = "Not saved — another interface changed this draft. Your version remains here.";
            }
            if (current && !(m_shutdown && !m_draftDirty)) {
                cancelShutdown(); fail(m_attachmentImportStatus[key]);
            }
            emit draftChanged();
            if (current && !uncertain && !m_draftConflict) saveDraft();
            if (m_shutdown) finishShutdown();
            return;
        }
        m_attachmentImports.remove(key); m_attachmentImportRetry.remove(key);
        m_attachmentImportStatus[key] = "Attached " + attachment["name"].toString() + ".";
        if (current && m_draft.value("revision").toLongLong() == intent["expected_revision"].toInteger()) {
            // Preserve all newer local authored fields and attachment removals.
            // Only this import's new selection and durable revision are merged.
            auto ids = m_draft.value("attachment_ids").toList();
            if (!ids.contains(attachment["id"].toString())) ids.append(attachment["id"].toString());
            QVariantList files;
            for (const auto &value : draft["attachments"].toArray())
                if (ids.contains(value.toObject()["id"].toString())) files.append(value.toVariant());
            m_draft["attachment_ids"] = ids; m_draft["attachments"] = files;
            m_draft["revision"] = draft["revision"].toVariant();
            m_draft["updated_at"] = draft["updated_at"].toVariant();
            m_draftDirty = m_draftDirty || ids != draft["attachment_ids"].toArray().toVariantList();
        }
        // A newer projection or another account/draft is never replaced by the
        // original receipt, including a replay returned after navigation.
        if (current) mergeGoogleDraftStatus(draft["google"].toObject(), intent["account"].toString(), intent["draft_id"].toString());
        if (current) mergeGoogleSendStatus(draft["google_send"].toObject(), intent["account"].toString(), intent["draft_id"].toString());
        emit draftChanged();
        if (current) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}


QUrl MailBackend::attachmentSaveLocation(const QString &name) const {
    auto filename = QFileInfo(QString(name).replace('\\', '/')).fileName();
    if (filename.isEmpty() || filename == "." || filename == "..") filename = "attachment";
    return QUrl::fromLocalFile(QDir(QStandardPaths::writableLocation(QStandardPaths::DownloadLocation)).filePath(filename));
}
// A stored attachment's local copy, or a remote thread's attachment fetched from the server.
void MailBackend::attachmentFile(const QString &account, const QString &id, Callback callback) {
    if (id.startsWith(u"remote-att:")) googleOperation("google_remote_attachment", account, {{"id", id}}, std::move(callback));
    else request("attachment", account, {{"id", id}}, std::move(callback));
}

QString MailBackend::requestAttachmentPreview(const QString &account, const QString &id) {
    const auto ticket = QUuid::createUuid().toString(QUuid::WithoutBraces);
    QTimer::singleShot(0, this, [this, ticket, account, id] {
        attachmentFile(account, id, [this, ticket, account, id](auto result, bool ok) {
            const auto path = result["path"].toString();
            ok = ok && result["account"] == account && result["id"] == id && QDir::isAbsolutePath(path) && QFileInfo(path).isFile();
            if (ok) {
                const auto mime = result["mime_type"].toString();
                const auto suffix = QFileInfo(path).suffix().toLower();
                // A file sent as a generic type is still a video by its name.
                const bool video = QStringList{"mp4","webm","mov","m4v","mkv","avi"}.contains(suffix)
                    && (mime.startsWith("video/") || mime.isEmpty() || mime == "application/octet-stream");
                QImageReader reader(path);
                const auto format = reader.format().toLower();
                auto size = reader.size();
                if (reader.transformation() & QImageIOHandler::TransformationRotate90) size.transpose();
                const bool raster = QList<QByteArray>{"png","jpeg","jpg","gif","webp","bmp","ico","tiff","tif"}.contains(format);
                if (video) {
                    result["preview_url"] = QUrl::fromLocalFile(path).toString();
                    result["video"] = true;
                } else if (raster && size.isValid() && qint64(size.width()) * size.height() <= 64000000) {
                    result["preview_url"] = QUrl::fromLocalFile(path).toString();
                    result["width"] = size.width(); result["height"] = size.height();
                } else result["preview_error"] = "Preview unavailable. You can still save this file.";
            } else result = {{"message", "This attachment is not available on this machine yet."}};
            emit attachmentPreviewReady(ticket, result.toVariantMap(), ok);
        });
    });
    return ticket;
}
// A released player clears its output, so a video card keeps the frame its
// decoder showed as a small JPEG for this run. A card that comes back into view
// shows it again without decoding.
QVariantMap MailBackend::keepVideoPoster(QObject *sink, const QString &account, const QString &id, qint64 duration) {
    const auto *video = qobject_cast<QVideoSink *>(sink);
    if (!video || account.isEmpty() || id.isEmpty()) return {};
    auto image = video->videoFrame().toImage();
    if (image.isNull()) return {};
    if (image.width() > 960 || image.height() > 960) image = image.scaled(960, 960, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    if (!m_posterDirectory) {
        const QDir cache(QDir(m_profile).filePath("cache"));
        m_posterDirectory = std::make_unique<QTemporaryDir>(!m_profile.isEmpty() && QDir().mkpath(cache.path()) ? cache.filePath("video-posters-XXXXXX") : QString{});
    }
    if (!m_posterDirectory->isValid()) return {};
    const auto key = account + '\n' + id;
    const auto path = m_posterDirectory->filePath(QString::fromLatin1(QCryptographicHash::hash(key.toUtf8(), QCryptographicHash::Sha256).toHex()) + ".jpg");
    if (!image.convertToFormat(QImage::Format_RGB32).save(path, "JPEG", 88)) return {};
    const QVariantMap poster{{"url", QUrl::fromLocalFile(path).toString()}, {"duration", duration}};
    m_videoPosters.insert(key, poster);
    return poster;
}
QString MailBackend::downloadAttachment(const QString &account, const QString &id, const QString &name) {
    const QFileInfo original(attachmentSaveLocation(name).toLocalFile());
    const auto directory = original.absoluteDir();
    if (!directory.exists() && !QDir().mkpath(directory.absolutePath())) {
        showActionToast("Could not open Downloads.", {}, {}, 6000); return {};
    }
    // Claim a free name by creating the file exclusively (NewOnly), so an
    // existing download is never overwritten and two saves cannot pick the
    // same name. Stop early if creation fails for any reason but a clash.
    QString destination;
    for (int n = 0; n < 10000; ++n) {
        const auto filename = n == 0 ? original.fileName() : original.completeBaseName() + " (" + QString::number(n) + ")" + (original.suffix().isEmpty() ? "" : "." + original.suffix());
        const auto candidate = directory.filePath(filename);
        QFile reservation(candidate);
        if (reservation.open(QIODevice::WriteOnly | QIODevice::NewOnly)) { destination = candidate; break; }
        if (!QFileInfo::exists(candidate)) break;
    }
    if (destination.isEmpty()) { showActionToast("Could not save this attachment to Downloads.", {}, {}, 6000); return {}; }
    const auto ticket = saveAttachment(account, id, QUrl::fromLocalFile(destination));
    const auto connection = std::make_shared<QMetaObject::Connection>();
    *connection = connect(this, &MailBackend::attachmentSaved, this, [this, ticket, destination, connection](const QString &reply, const QString &error) {
        if (reply != ticket) return;
        disconnect(*connection);
        if (!error.isEmpty()) {
            if (QFileInfo(destination).size() == 0) QFile::remove(destination);
            showActionToast(error, {}, {}, 6000);
        } else showActionToast("Saved " + QFileInfo(destination).fileName() + " to Downloads", {}, {}, 6000);
    });
    return ticket;
}
QString MailBackend::saveAttachment(const QString &account, const QString &id, const QUrl &destination) {
    const auto ticket = QUuid::createUuid().toString(QUuid::WithoutBraces);
    QTimer::singleShot(0, this, [this, ticket, account, id, destination] {
        if (!destination.isLocalFile() || !QDir::isAbsolutePath(destination.toLocalFile()) || m_shutdown) {
            emit attachmentSaved(ticket, "Choose a local file to save this attachment."); return;
        }
        if (findChildren<QTimer *>("attachmentCopy").size() >= 4) { emit attachmentSaved(ticket, "Wait for another attachment to finish saving."); return; }
        attachmentFile(account, id, [this, ticket, account, id, destination](auto result, bool ok) {
            if (!ok || result["account"] != account || result["id"] != id || !QDir::isAbsolutePath(result["path"].toString())
                || result["sha256"].toString().size() != 64 || result["size"].toInteger(-1) < 0) {
                emit attachmentSaved(ticket, "This attachment could not be saved. Its local copy is unavailable."); return;
            }
            if (m_shutdown || findChildren<QTimer *>("attachmentCopy").size() >= 4) { emit attachmentSaved(ticket, "Wait for another attachment to finish saving."); return; }
            // Copy 256 KiB per event-loop turn so large files never freeze the
            // GUI, check size and SHA-256 against the engine's record, and write
            // through QSaveFile so a failed copy leaves the destination as it was.
            // The named timers double as the count of concurrent copies (max 4).
            struct Copy {
                QFile source; QSaveFile output; QCryptographicHash hash{QCryptographicHash::Sha256};
                qint64 bytes = 0, expected = 0; QByteArray digest;
                Copy(const QString &from, const QString &to) : source(from), output(to) {}
            };
            auto copy = std::make_shared<Copy>(result["path"].toString(), destination.toLocalFile());
            copy->expected = result["size"].toInteger(); copy->digest = result["sha256"].toString().toLatin1();
            if (!copy->source.open(QIODevice::ReadOnly) || !copy->output.open(QIODevice::WriteOnly)) {
                emit attachmentSaved(ticket, "Could not save here. Choose another location and try again."); return;
            }
            auto timer = new QTimer(this); timer->setObjectName("attachmentCopy"); timer->setInterval(0);
            connect(timer, &QTimer::timeout, this, [this, timer, copy, ticket] {
                auto finish = [this, timer, ticket](const QString &error) { timer->stop(); timer->deleteLater(); emit attachmentSaved(ticket, error); };
                if (m_shutdown) { copy->output.cancelWriting(); finish("Saving stopped because the app is closing."); return; }
                const auto bytes = copy->source.read(256 * 1024);
                if (copy->source.error() != QFileDevice::NoError || copy->output.write(bytes) != bytes.size()) {
                    copy->output.cancelWriting(); finish("Could not finish saving. The destination file was left unchanged."); return;
                }
                copy->hash.addData(bytes); copy->bytes += bytes.size();
                if (copy->bytes > copy->expected || (copy->source.atEnd() && (copy->bytes != copy->expected || copy->hash.result().toHex() != copy->digest))) {
                    copy->output.cancelWriting(); finish("The attachment failed verification. The destination file was left unchanged."); return;
                }
                if (copy->source.atEnd()) {
                    const bool saved = copy->output.commit();
                    finish(saved ? QString{} : "Could not finish saving. Try another location.");
                }
            });
            timer->start();
        });
    });
    return ticket;
}
