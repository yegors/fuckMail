#pragma once
#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QUrl>
#include <QVariantList>
#include <QVariantMap>
#include <functional>

class GoogleOperation;
class MailBackend;

/// The OpenPGP keyring for Settings → Encryption, the account cards and the
/// composer: lists keys, creates, imports, exports, renews and deletes them,
/// sets each account's key, and checks recipients. Each call uses its own
/// engine connection, as making an RSA key can take a while.
///
/// MailBackend creates one lazily (its `pgpKeys` property). GUI thread only.
/// Passphrases pass through only on their way to the engine, and nothing here
/// keeps them; exported secret keys never come here, as the engine writes them
/// to the file the person chose.
class PgpKeys final : public QObject {
    Q_OBJECT
    /// The engine's `pgp_keys` list: one map per key, own keys first.
    Q_PROPERTY(QVariantList keys READ keys NOTIFY changed)
    /// account id → {fingerprint, sign_by_default, encrypt_by_default}.
    Q_PROPERTY(QVariantMap accounts READ accounts NOTIFY changed)
    /// True until the first list arrives.
    Q_PROPERTY(bool loading READ loading NOTIFY changed)
    /// The action in progress (generate, import, export, expiry, delete,
    /// account), or "".
    Q_PROPERTY(QString busy READ busy NOTIFY busyChanged)
public:
    explicit PgpKeys(MailBackend *backend);
    ~PgpKeys() override;
    QVariantList keys() const { return m_keys; }
    QVariantMap accounts() const { return m_accounts; }
    bool loading() const { return m_loading; }
    QString busy() const { return m_busy; }

    /// Reloads the list; `changed` follows.
    Q_INVOKABLE void refresh();
    /// The key with this fingerprint from the list, or an empty map.
    Q_INVOKABLE QVariantMap key(const QString &fingerprint) const;
    /// Makes a key for `account` (its name and address) and makes it the
    /// account's key. `algorithm` is curve25519 or rsa4096; `expiresDays` 0
    /// means it never expires. Returns a ticket for `finished`.
    Q_INVOKABLE QString generate(const QString &account, const QString &name, const QString &email,
                                 const QString &algorithm, int expiresDays);
    /// Imports pasted armored text, or with `file` set, that local file. A
    /// protected secret key fails with `passphrase_required` (details name the
    /// key) until `passphrase` unlocks it. A key naming an address another key
    /// has fails with `pgp_key_conflict` (details list them) unless `replace`.
    Q_INVOKABLE QString importKeys(const QString &text, const QUrl &file, const QString &passphrase, bool replace);
    /// Reads the public keys in a message's attachment, such as a key someone
    /// sent, without storing them: `previewReady(ticket, …)` follows with the
    /// engine's preview (keys, revoked, conflicts) and the file's `path`. A
    /// secret key in it fails with `secret_key_not_allowed`.
    Q_INVOKABLE QString previewAttachment(const QString &account, const QString &attachment);
    /// Imports the public keys previewed at `path`; `replace` lets them take
    /// addresses other keys have. `finished(ticket, "import", …)` follows.
    Q_INVOKABLE QString importPreviewed(const QString &path, bool replace);
    /// Exports a key: `kind` is public, secret (protected with `passphrase`)
    /// or revocation. With `destination`, the engine writes it there, readable
    /// by this user alone, replacing a file the save dialog asked about;
    /// without, a public key is copied to the clipboard.
    Q_INVOKABLE QString exportKey(const QString &fingerprint, const QString &kind, const QString &passphrase,
                                  const QUrl &destination);
    /// Moves an own key's expiry `days` from now; 0 removes it.
    Q_INVOKABLE QString setExpiry(const QString &fingerprint, int days);
    Q_INVOKABLE QString remove(const QString &fingerprint);
    /// Points `account` at one of its owner's keys, or with an empty
    /// fingerprint stops using OpenPGP for it.
    Q_INVOKABLE QString useForAccount(const QString &account, const QString &fingerprint, bool sign, bool encrypt);
    /// Asks whether mail from `account` to `addresses` can be encrypted;
    /// `recipientsReady(ticket, result, ok)` follows with the engine's
    /// `pgp_recipients` result.
    Q_INVOKABLE QString checkRecipients(const QString &account, const QStringList &addresses);
    /// A fingerprint in groups of four, as people compare them.
    Q_INVOKABLE QString grouped(const QString &fingerprint) const;
    /// Where the save dialog starts: the folder last saved to, else Documents.
    Q_INVOKABLE QUrl exportFolder() const;
    /// The suggested file for exporting `key` as `kind` (public, secret or
    /// revocation): its short key ID and the kind, in exportFolder().
    Q_INVOKABLE QUrl exportFile(const QVariantMap &key, const QString &kind) const;
signals:
    void changed();
    void busyChanged();
    /// Answers each mutation and export. `result` is the engine's result, or
    /// {code, message, details} when `ok` is false.
    void finished(const QString &ticket, const QString &action, const QVariantMap &result, bool ok);
    void recipientsReady(const QString &ticket, const QVariantMap &result, bool ok);
    void previewReady(const QString &ticket, const QVariantMap &result, bool ok);
    /// Keys or account choices changed, so decrypted views and signature
    /// checks shown earlier may be out of date.
    void keyringChanged();
private:
    using Callback = std::function<void(const QJsonObject &, bool)>;
    void call(const QString &account, const QString &method, const QJsonObject &params, Callback callback);
    QString mutate(const QString &action, const QString &account, const QString &method, QJsonObject params);
    void send(const QString &ticket, const QString &action, const QString &account, const QString &method, QJsonObject params);
    void setBusy(const QString &busy);
    MailBackend *m_backend;
    QList<GoogleOperation *> m_operations;
    QVariantList m_keys;
    QVariantMap m_accounts;
    QString m_busy;
    QString m_exportFolder;
    bool m_loading = true;
    int m_serial = 0;
    int m_refreshGeneration = 0;
};
