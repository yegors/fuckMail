// PgpKeys: the OpenPGP keyring's engine calls for Settings → Encryption, the
// account cards and the composer, each on its own GoogleOperation connection.
#include "PgpKeys.h"
#include "GoogleOperation.h"
#include "HtmlContent.h"
#include "MailBackend.h"
#include <QFile>
#include <QJsonArray>
#include <QSaveFile>
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
        m_keys = keys;
        m_accounts = result["accounts"].toObject().toVariantMap();
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
    params["request_id"] = QUuid::createUuid().toString(QUuid::WithoutBraces);
    setBusy(action);
    call(account, method, params, [this, ticket, action](const QJsonObject &result, bool ok) {
        setBusy({});
        if (ok) { refresh(); emit keyringChanged(); m_backend->loadAccounts(); }
        emit finished(ticket, action, result.toVariantMap(), ok);
    });
    return ticket;
}

QString PgpKeys::generate(const QString &account, const QString &name, const QString &email, const QString &algorithm, int expiresDays) {
    QJsonObject params{{"name", name.trimmed()}, {"email", email.trimmed()}, {"algorithm", algorithm},
                       {"expires_days", expiresDays > 0 ? QJsonValue(expiresDays) : QJsonValue(QJsonValue::Null)}};
    if (!account.isEmpty()) params["account"] = account;
    return mutate("generate", {}, "pgp_key_generate", params);
}

QString PgpKeys::importKeys(const QString &text, const QUrl &file, const QString &passphrase) {
    QJsonObject params;
    if (file.isLocalFile()) params["path"] = file.toLocalFile();
    else params["text"] = text;
    if (!passphrase.isEmpty()) params["passphrase"] = passphrase;
    return mutate("import", {}, "pgp_key_import", params);
}

QString PgpKeys::exportKey(const QString &fingerprint, const QString &kind, const QString &passphrase, const QUrl &destination) {
    const auto ticket = QStringLiteral("pgp-%1").arg(++m_serial);
    QJsonObject params{{"fingerprint", fingerprint}, {"kind", kind}};
    if (kind == "secret") params["passphrase"] = passphrase;
    setBusy("export");
    call({}, "pgp_key_export", params, [this, ticket, destination](const QJsonObject &result, bool ok) {
        setBusy({});
        auto reply = result.toVariantMap();
        if (ok) {
            const auto armored = result["armored"].toString().toUtf8();
            reply.remove("armored");
            if (destination.isLocalFile()) {
                // A new private file the person named; a secret key is never
                // left readable by others, even briefly.
                QSaveFile file(destination.toLocalFile());
                if (!file.open(QIODevice::WriteOnly) || !file.setPermissions(QFileDevice::ReadOwner | QFileDevice::WriteOwner)
                    || file.write(armored) != armored.size() || !file.commit()) {
                    ok = false;
                    reply = {{"code", "storage_error"}, {"message", "The key couldn't be saved there."}};
                } else {
                    reply["path"] = destination.toLocalFile();
                }
            } else {
                m_backend->copyText(QString::fromUtf8(armored));
                reply["copied"] = true;
            }
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

QString PgpKeys::grouped(const QString &fingerprint) const {
    QStringList groups;
    for (qsizetype i = 0; i < fingerprint.size(); i += 4) groups << fingerprint.mid(i, 4);
    return groups.join(' ');
}
