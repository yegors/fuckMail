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
#include <QUuid>
#include <QDateTime>
#include <QImage>
#include <QPointer>
#include <QRegularExpression>
#include <QSet>
#include <QThreadPool>
#include <algorithm>
#include <memory>
#if defined(Q_OS_MACOS)
#include <sys/xattr.h>
#endif

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
    queueDraftAttachments(files, account, draftId, false);
}

void MailBackend::attachPastedFiles(const QList<QUrl> &files, const QString &account, const QString &draftId) {
    queueDraftAttachments(files, account, draftId, true);
}

QString MailBackend::pastedImageName(const QDir &folder, const QDateTime &time) {
    const auto stem = "Screenshot " + time.toString("yyyy-MM-dd 'at' HH.mm.ss");
    auto name = stem + ".png";
    for (int copy = 2; folder.exists(name); ++copy) name = QString("%1 (%2).png").arg(stem).arg(copy);
    return name;
}

void MailBackend::attachPastedImage(const QImage &image, const QString &account, const QString &draftId) {
    if (image.isNull() || account != m_account || draftId.isEmpty() || draftId != m_draft.value("draft_id").toString()) return;
    const auto key = importKey(account, draftId);
    QDir folder(QDir(m_profile).filePath("cache/pasted"));
    if (!folder.mkpath(".")) { m_attachmentImportStatus[key] = "Not attached — the pasted image could not be saved."; emit draftChanged(); return; }
    // Retry reads the file again, so pasted images stay a week, then go.
    const auto expired = QDateTime::currentDateTime().addDays(-7);
    for (const auto &old : folder.entryInfoList(QDir::Files))
        if (old.lastModified() < expired) QFile::remove(old.filePath());
    const auto path = folder.filePath(pastedImageName(folder, QDateTime::currentDateTime()));
    // Reserve the name now, so a second paste in the same second picks the next one.
    if (QFile reserve(path); !reserve.open(QIODevice::WriteOnly)) { m_attachmentImportStatus[key] = "Not attached — the pasted image could not be saved."; emit draftChanged(); return; }
    m_attachmentImportStatus[key] = "Preparing pasted image…"; emit draftChanged();
    // Encoding a large screenshot takes a moment; keep it off the UI thread.
    QPointer<MailBackend> self(this);
    QThreadPool::globalInstance()->start([self, image, path, account, draftId] {
        QSaveFile file(path);
        const bool saved = file.open(QIODevice::WriteOnly) && image.save(&file, "PNG") && file.commit();
        QMetaObject::invokeMethod(self, [self, saved, path, account, draftId] {
            if (!self) return;
            if (!saved) {
                QFile::remove(path);
                self->m_attachmentImportStatus[importKey(account, draftId)] = "Not attached — the pasted image could not be saved.";
                emit self->draftChanged(); return;
            }
            self->queueDraftAttachments({QUrl::fromLocalFile(path)}, account, draftId, true);
        }, Qt::QueuedConnection);
    });
}

void MailBackend::queueDraftAttachments(const QList<QUrl> &files, const QString &account, const QString &draftId, bool joinPending) {
    // The native picker captures its origin before opening. A later selection
    // cannot attach to whichever draft happens to be displayed by then.
    if (account != m_account || draftId.isEmpty() || draftId != m_draft.value("draft_id").toString()) return;
    const auto key = importKey(account, draftId);
    auto reject = [this, key](const QString &message) { m_attachmentImportStatus[key] = message; emit draftChanged(); };
    if (!m_connected || m_shutdown || draftLocked()) { reject("Reconnect and finish the current draft operation before attaching a file."); return; }
    if (m_draftConflict) { reject("Resolve the saved draft conflict before attaching a file. Your edits remain here."); return; }
    if (files.isEmpty()) return;
    const auto waiting = m_attachmentQueues.value(key).size() + (m_attachmentImports.contains(key) ? 1 : 0);
    if (files.size() + waiting + m_draft.value("attachment_ids").toList().size() > 50) { reject("A draft supports at most 50 attachments."); return; }
    if (attachmentImportPending()) {
        // A paste joins files still importing normally; a stopped or failed
        // import is resolved first, as for chosen files.
        if (!joinPending || m_attachmentQueuePaused.contains(key) || m_attachmentImportRetry.contains(key)) {
            reject("Finish or skip the pending files before choosing more."); return;
        }
        for (const auto &file : files)
            if (!file.isLocalFile() || !QDir::isAbsolutePath(file.toLocalFile())) { reject("Choose local files using absolute paths."); return; }
        auto &queue = m_attachmentQueues[key];
        if (!m_attachmentQueueLifecycle.contains(key))
            m_attachmentQueueLifecycle[key] = lifecycleAccount(account).value("lifecycle").toMap().value("revision").toLongLong();
        for (const auto &file : files) if (!queue.contains(file)) queue.append(file);
        emit draftChanged();
        advanceDraftAttachments();
        return;
    }
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


// Marks a saved attachment as downloaded from the internet, as browsers do, so
// the system checks it before anything in it runs: the quarantine flag on
// macOS (Gatekeeper), the Internet zone on Windows (SmartScreen, Office
// Protected View). A mark already there is kept with any approval it holds.
static void markDownloaded(const QString &path) {
#if defined(Q_OS_MACOS)
    const auto file = QFile::encodeName(path);
    if (getxattr(file.constData(), "com.apple.quarantine", nullptr, 0, 0, XATTR_NOFOLLOW) >= 0) return;
    const auto value = QStringLiteral("0081;%1;fuckMail;").arg(QDateTime::currentSecsSinceEpoch(), 8, 16, QChar('0')).toLatin1();
    setxattr(file.constData(), "com.apple.quarantine", value.constData(), size_t(value.size()), 0, XATTR_NOFOLLOW);
#elif defined(Q_OS_WIN)
    QFile zone(path + QStringLiteral(":Zone.Identifier"));
    if (zone.open(QIODevice::WriteOnly | QIODevice::Truncate)) zone.write("[ZoneTransfer]\r\nZoneId=3\r\n");
#else
    Q_UNUSED(path);
#endif
}

// Mirrors the engine's export-name rules but keeps the sender's readable name:
// no folders, no leading dot (a hidden startup file such as .bash_profile), no
// control, bidi or other invisible characters (which can show "exe.pdf" for a
// program), none of \ / : * ? " < > | (a colon names an NTFS stream), no
// trailing dot or space, no Windows device name, and a length that leaves room
// for " (2)" within the 255 bytes file systems allow.
QString MailBackend::safeDownloadName(const QString &name) {
    static const QString forbidden = QStringLiteral("\\/:*?\"<>|");
    const auto base = name.mid(std::max(name.lastIndexOf('/'), name.lastIndexOf('\\')) + 1);
    QString clean;
    for (const char32_t code : base.toUcs4()) {
        const auto category = QChar::category(code);
        const bool unsafe = category == QChar::Other_Control || category == QChar::Other_Format || category == QChar::Other_Surrogate
            || category == QChar::Separator_Line || category == QChar::Separator_Paragraph || (code < 0x80 && forbidden.contains(QChar(char16_t(code))));
        clean += unsafe ? QStringLiteral("_") : QString::fromUcs4(&code, 1);
    }
    const auto trim = [](QString &text) {
        while (!text.isEmpty() && (text.back() == '.' || text.back().isSpace())) text.chop(1);
    };
    while (!clean.isEmpty() && (clean.front() == '.' || clean.front().isSpace())) clean.remove(0, 1);
    trim(clean);
    // Keep a short ordinary extension whole and shorten the part before it.
    const auto dot = clean.lastIndexOf('.');
    QString extension = dot > 0 ? clean.mid(dot) : QString();
    const bool ordinary = extension.size() >= 2 && extension.size() <= 17
        && std::all_of(extension.begin() + 1, extension.end(), [](QChar c) { return c.unicode() < 0x80 && c.isLetterOrNumber(); });
    if (!ordinary) extension.clear();
    QString stem;
    qsizetype bytes = extension.size();
    for (const char32_t code : clean.left(clean.size() - extension.size()).toUcs4()) {
        const auto character = QString::fromUcs4(&code, 1);
        bytes += character.toUtf8().size();
        if (bytes > 200) break;
        stem += character;
    }
    trim(stem);
    if (stem.isEmpty()) stem = "attachment";
    // Windows opens a device, not a file, for these names with any extension.
    static const QRegularExpression device(QStringLiteral("^(CON|PRN|AUX|NUL|CONIN\\$|CONOUT\\$|COM[0-9¹²³]|LPT[0-9¹²³])$"),
                                           QRegularExpression::CaseInsensitiveOption);
    if (device.match(stem.section('.', 0, 0).trimmed()).hasMatch()) stem.prepend('_');
    return stem + extension;
}
// Downloads, unless it is set to the home folder itself, where a saved file
// would sit beside the shell's and desktop's startup files: then ~/Downloads.
QUrl MailBackend::attachmentSaveLocation(const QString &name) const {
    const QDir home(QDir::homePath());
    QString downloads = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
    const auto canonical = QDir(downloads).canonicalPath();
    if (downloads.isEmpty() || QDir::cleanPath(QDir(downloads).absolutePath()) == QDir::cleanPath(home.absolutePath())
        || (!canonical.isEmpty() && canonical == home.canonicalPath()))
        downloads = home.filePath("Downloads");
    return QUrl::fromLocalFile(QDir(downloads).filePath(safeDownloadName(name)));
}
bool MailBackend::playableVideo(const QString &path) {
    // The media backend picks its demuxer from the bytes, not the name, so a
    // playlist or any other format renamed .mp4 would reach it otherwise.
    static const QSet<QString> isoMedia{"mp4", "m4v", "mov"}, matroska{"mkv", "webm"};
    const auto suffix = QFileInfo(path).suffix().toLower();
    QFile file(path);
    if ((!isoMedia.contains(suffix) && !matroska.contains(suffix)) || !file.open(QIODevice::ReadOnly)) return false;
    const auto head = file.read(12);
    if (isoMedia.contains(suffix)) return head.size() == 12 && head.mid(4, 4) == "ftyp";
    return head.startsWith("\x1a\x45\xdf\xa3");
}
// A stored attachment's local copy, or a remote thread's attachment fetched from the server.
void MailBackend::attachmentFile(const QString &account, const QString &id, Callback callback) {
    if (id.startsWith(u"remote-att:")) googleOperation("google_remote_attachment", account, {{"id", id}}, std::move(callback));
    else request("attachment", account, {{"id", id}}, std::move(callback));
}

QString MailBackend::requestAttachmentPreview(const QString &account, const QString &id, bool video) {
    const auto ticket = QUuid::createUuid().toString(QUuid::WithoutBraces);
    QTimer::singleShot(0, this, [this, ticket, account, id, video] {
        // A video plays from its local copy only once the person opens it, and
        // only an MP4, QuickTime, Matroska or WebM file.
        if (video) {
            attachmentFile(account, id, [this, ticket, account, id](auto result, bool ok) {
                const auto path = result["path"].toString();
                ok = ok && result["account"] == account && result["id"] == id && QDir::isAbsolutePath(path) && QFileInfo(path).isFile();
                if (ok && playableVideo(path)) { result["preview_url"] = QUrl::fromLocalFile(path).toString(); result["video"] = true; }
                else if (ok) { result = {{"account", account}, {"id", id}, {"message", "Preview unavailable. You can still save this file."}}; ok = false; }
                else result = {{"message", "This attachment is not available on this machine yet."}};
                emit attachmentPreviewReady(ticket, result.toVariantMap(), ok);
            });
            return;
        }
        // An image shows only the PNG the engine decoded and re-encoded, so the
        // sender's bytes never reach Qt's own image decoders.
        auto shown = [this, ticket, account, id](auto result, bool ok) {
            const auto path = result["preview_path"].toString();
            ok = ok && result["account"] == account && result["id"] == id;
            if (ok && QDir::isAbsolutePath(path) && QFileInfo(path).isFile()) result["preview_url"] = QUrl::fromLocalFile(path).toString();
            else if (ok) result["preview_error"] = "Preview unavailable. You can still save this file.";
            else result = {{"message", "This attachment is not available on this machine yet."}};
            emit attachmentPreviewReady(ticket, result.toVariantMap(), ok);
        };
        const QJsonObject params{{"id", id}, {"preview", true}};
        if (id.startsWith(u"remote-att:")) googleOperation("google_remote_attachment", account, params, std::move(shown));
        else request("attachment", account, params, std::move(shown));
    });
    return ticket;
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
                    if (saved) markDownloaded(copy->output.fileName());
                    finish(saved ? QString{} : "Could not finish saving. Try another location.");
                }
            });
            timer->start();
        });
    });
    return ticket;
}
