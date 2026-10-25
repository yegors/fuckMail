// PgpKeys: the OpenPGP keyring's engine calls for Settings → Encryption, the
// account cards and the composer, each on its own GoogleOperation connection.
#include "PgpKeys.h"
#include "GoogleOperation.h"
#include "HtmlContent.h"
#include "MailBackend.h"
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QStandardPaths>
#include <QUuid>

// MailBackend owns one instance, created when QML first asks for it.
QObject *MailBackend::pgpKeys() const {
    if (!m_pgpKeys) {
        auto backend = const_cast<MailBackend *>(this);
        m_pgpKeys = new PgpKeys(backend);
        // A new key may decrypt mail that couldn't be read, or verify a
        // signature that was unknown: read sources and HTML again.
        QObject::connect(m_pgpKeys, &PgpKeys::keyringChanged, backend, [backend] {
            backend->m_sourceCache.clear();
            if (backend->m_htmlContent) backend->m_htmlContent->forgetViews();
        });
    }
    return m_pgpKeys;
}

PgpKeys::PgpKeys(MailBackend *backend) : QObject(backend), m_backend(backend) {
    // Account rows carry each account's key; when they change (here or from
    // the CLI) the list may have too.
    connect(backend, &MailBackend::accountsChanged, this, [this] { if (!m_loading) refresh(); });
    connect(backend, &MailBackend::connectedChanged, this, [this] { if (m_backend->connected()) refresh(); });
    refresh();
}

PgpKeys::~PgpKeys() {
    for (auto operation : std::as_const(m_operations)) operation->cancel(false);
}

void PgpKeys::call(const QString &account, const QString &method, const QJsonObject &params, Callback callback) {
    if (!m_backend->m_connected || m_backend->m_enginePid <= 0) {
        QMetaObject::invokeMethod(this, [callback] { callback({{"code", "engine_disconnected"}, {"message", "The local engine is not connected."}}, false); }, Qt::QueuedConnection);
        return;
    }
    auto operation = new GoogleOperation(account, {}, this);
    m_operations.append(operation);
    // Results always arrive after the caller has its ticket.
    operation->start(m_backend->m_profile, m_backend->m_enginePid, method, params, [this, operation, callback = std::move(callback)](auto result, bool ok) {
        m_operations.removeOne(operation);
        operation->deleteLater();
        QMetaObject::invokeMethod(this, [callback, result, ok] { callback(result, ok); }, Qt::QueuedConnection);
    });
}

void PgpKeys::setBusy(const QString &busy) {
    if (m_busy == busy) return;
    m_busy = busy;
    emit busyChanged();
}

void PgpKeys::refresh() {
    const int generation = ++m_refreshGeneration;
    call({}, "pgp_keys", {}, [this, generation](const QJsonObject &result, bool ok) {
        if (generation != m_refreshGeneration || !ok) return;
        auto keys = result["keys"].toArray().toVariantList();
        // The person's own keys first, then other people's, each oldest first.
        std::stable_sort(keys.begin(), keys.end(), [](const QVariant &a, const QVariant &b) {
            return a.toMap().value("secret").toBool() && !b.toMap().value("secret").toBool();
        });
        auto accounts = result["accounts"].toObject().toVariantMap();
        // Each change reloads the list more than once (here, then as the
        // account rows change); an unchanged list rebuilds nothing.
        if (!m_loading && keys == m_keys && accounts == m_accounts) return;
        m_keys = keys;
        m_accounts = accounts;
        m_loading = false;
        emit changed();
    });
}

QVariantMap PgpKeys::key(const QString &fingerprint) const {
    for (const auto &value : m_keys)
        if (value.toMap().value("fingerprint").toString().compare(fingerprint, Qt::CaseInsensitive) == 0) return value.toMap();
    return {};
}

QString PgpKeys::mutate(const QString &action, const QString &account, const QString &method, QJsonObject params) {
    const auto ticket = QStringLiteral("pgp-%1").arg(++m_serial);
    setBusy(action);
    send(ticket, action, account, method, std::move(params));
    return ticket;
}

void PgpKeys::send(const QString &ticket, const QString &action, const QString &account, const QString &method, QJsonObject params) {
    params["request_id"] = QUuid::createUuid().toString(QUuid::WithoutBraces);
    call(account, method, params, [this, ticket, action](const QJsonObject &result, bool ok) {
        setBusy({});
        if (ok) {
            refresh();
            // An account's key and defaults decide nothing about reading, so
            // held messages stay as they are; the account rows show the choice.
            if (action != "account") emit keyringChanged();
            m_backend->loadAccounts();
        }
        emit finished(ticket, action, result.toVariantMap(), ok);
    });
}

QString PgpKeys::previewAttachment(const QString &account, const QString &attachment) {
    const auto ticket = QStringLiteral("pgp-%1").arg(++m_serial);
    // The engine verifies the attachment and hands back its local copy, then
    // reads the keys in it without storing anything.
    m_backend->attachmentFile(account, attachment, [this, ticket](const QJsonObject &result, bool ok) {
        const auto path = result["path"].toString();
        if (!ok || path.isEmpty()) {
            emit previewReady(ticket, {{"code", ok ? "content_unavailable" : result["code"].toString()},
                                       {"message", result["message"].toString("This key isn't on this computer yet.")}}, false);
            return;
        }
        call({}, "pgp_key_import", {{"path", path}, {"preview", true}, {"public_only", true}}, [this, ticket, path](const QJsonObject &result, bool ok) {
            auto reply = result.toVariantMap();
            if (ok) reply["path"] = path;
            emit previewReady(ticket, reply, ok);
        });
    });
    return ticket;
}

QString PgpKeys::importPreviewed(const QString &path, bool replace) {
    return mutate("import", {}, "pgp_key_import", {{"path", path}, {"public_only", true}, {"replace", replace}});
}

QString PgpKeys::generate(const QString &account, const QString &name, const QString &email, const QString &algorithm, int expiresDays) {
    QJsonObject params{{"name", name.trimmed()}, {"email", email.trimmed()}, {"algorithm", algorithm},
                       {"expires_days", expiresDays > 0 ? QJsonValue(expiresDays) : QJsonValue(QJsonValue::Null)}};
    if (!account.isEmpty()) params["account"] = account;
    return mutate("generate", {}, "pgp_key_generate", params);
}

QString PgpKeys::importKeys(const QString &text, const QUrl &file, const QString &passphrase, bool replace) {
    QJsonObject params;
    if (file.isLocalFile()) params["path"] = file.toLocalFile();
    else params["text"] = text;
    if (!passphrase.isEmpty()) params["passphrase"] = passphrase;
    if (replace) params["replace"] = true;
    return mutate("import", {}, "pgp_key_import", params);
}

QString PgpKeys::exportKey(const QString &fingerprint, const QString &kind, const QString &passphrase, const QUrl &destination) {
    const auto ticket = QStringLiteral("pgp-%1").arg(++m_serial);
    QJsonObject params{{"fingerprint", fingerprint}, {"kind", kind}};
    if (kind == "secret") params["passphrase"] = passphrase;
    // The engine writes the file, readable by this user alone on every
    // platform; the save dialog has already asked before replacing one.
    if (destination.isLocalFile()) {
        params["path"] = QFileInfo(destination.toLocalFile()).absoluteFilePath();
        params["replace"] = true;
    }
    setBusy("export");
    call({}, "pgp_key_export", params, [this, ticket, destination](const QJsonObject &result, bool ok) {
        setBusy({});
        auto reply = result.toVariantMap();
        if (ok && destination.isLocalFile()) {
            m_exportFolder = QFileInfo(destination.toLocalFile()).absolutePath();
        } else if (ok) {
            m_backend->copyText(result["armored"].toString());
            reply.remove("armored");
            reply["copied"] = true;
        }
        emit finished(ticket, "export", reply, ok);
    });
    return ticket;
}

QString PgpKeys::setExpiry(const QString &fingerprint, int days) {
    return mutate("expiry", {}, "pgp_key_expiry", {{"fingerprint", fingerprint}, {"expires_days", days > 0 ? QJsonValue(days) : QJsonValue(QJsonValue::Null)}});
}

QString PgpKeys::remove(const QString &fingerprint) {
    return mutate("delete", {}, "pgp_key_delete", {{"fingerprint", fingerprint}});
}

QString PgpKeys::useForAccount(const QString &account, const QString &fingerprint, bool sign, bool encrypt) {
    return mutate("account", account, "pgp_account", {{"fingerprint", fingerprint.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(fingerprint)},
                                                     {"sign_by_default", sign}, {"encrypt_by_default", encrypt}});
}

QString PgpKeys::checkRecipients(const QString &account, const QStringList &addresses) {
    const auto ticket = QStringLiteral("pgp-%1").arg(++m_serial);
    call(account, "pgp_recipients", {{"addresses", QJsonArray::fromStringList(addresses)}}, [this, ticket](const QJsonObject &result, bool ok) {
        emit recipientsReady(ticket, result.toVariantMap(), ok);
    });
    return ticket;
}

QUrl PgpKeys::exportFolder() const {
    const auto folder = m_exportFolder.isEmpty() ? QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation) : m_exportFolder;
    return QUrl::fromLocalFile(folder);
}

QUrl PgpKeys::exportFile(const QVariantMap &key, const QString &kind) const {
    // The short key ID people quote: the last eight digits of the key ID.
    auto id = key.value("key_id").toString();
    if (id.isEmpty()) id = key.value("fingerprint").toString().right(16);
    return QUrl::fromLocalFile(QDir(exportFolder().toLocalFile()).filePath(id.right(8) + "-" + kind + ".asc"));
}

QString PgpKeys::grouped(const QString &fingerprint) const {
    QStringList groups;
    for (qsizetype i = 0; i < fingerprint.size(); i += 4) groups << fingerprint.mid(i, 4);
    return groups.join(' ');
}
