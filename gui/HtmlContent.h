#pragma once
#include <QHash>
#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QQueue>
#include <QTimer>
#include <QVariantMap>
#include <functional>

class GoogleOperation;
class MailBackend;

// The HTML reader's engine calls: sanitised message HTML, image fetches, link
// counts, the blocklist and the HTTPS proxy settings. Each call uses its own
// connection, so a slow image never holds the mail pipe.
class HtmlContent final : public QObject {
    Q_OBJECT
    // Blocked patterns, newest first: [{pattern, source, added_at}].
    Q_PROPERTY(QVariantList blockedHosts READ blockedHosts NOTIFY blockedHostsChanged)
    // pattern → messages linking to it, plus `complete` while the index catches up.
    Q_PROPERTY(QVariantMap blockCounts READ blockCounts NOTIFY blockCountsChanged)
    // {enabled, host, port, username, password_saved, testing, test}.
    Q_PROPERTY(QVariantMap proxy READ proxy NOTIFY proxyChanged)
    // {bytes, limit} of the on-disk image cache.
    Q_PROPERTY(QVariantMap imageCache READ imageCache NOTIFY imageCacheChanged)
    // Changes whenever link summaries or the blocklist change.
    Q_PROPERTY(int revision READ revision NOTIFY revisionChanged)
    // Changes when sender pictures arrive.
    Q_PROPERTY(int pictureRevision READ pictureRevision NOTIFY pictureRevisionChanged)
public:
    explicit HtmlContent(MailBackend *backend);
    ~HtmlContent() override;
    QVariantList blockedHosts() const;
    QVariantMap blockCounts() const { return m_blockCounts; }
    QVariantMap proxy() const;
    QVariantMap imageCache() const { return m_imageCache; }
    int revision() const { return m_revision; }
    int pictureRevision() const { return m_pictureRevision; }

    // Returns a ticket; `htmlReady(ticket, result, ok)` follows.
    Q_INVOKABLE QString load(const QString &account, const QString &id, const QString &sourceVersion);
    // A result already held, with the images loaded since; empty otherwise.
    Q_INVOKABLE QVariantMap cached(const QString &account, const QString &id, const QString &sourceVersion);
    // Returns a ticket; `imageReady(ticket, result, ok)` follows.
    Q_INVOKABLE QString loadImage(const QString &account, const QString &id, const QString &sourceVersion,
                                  int resource, bool cachedOnly = false);
    // Drops queued and running image work for one message.
    Q_INVOKABLE void cancelMessage(const QString &account, const QString &id);

    Q_INVOKABLE QString normalizePattern(const QString &input) const;
    Q_INVOKABLE QString blockedBy(const QString &host) const;
    Q_INVOKABLE bool linkBlocked(const QString &href) const;
    // Adds a pattern; returns an error to show, or "".
    Q_INVOKABLE QString block(const QString &input, const QString &source);
    Q_INVOKABLE void unblock(const QString &pattern);
    Q_INVOKABLE bool imagesAllowed(const QString &account, const QString &email) const;
    Q_INVOKABLE void allowImages(const QString &account, const QString &email);
    Q_INVOKABLE void disallowImages(const QString &account, const QString &email);

    // Cached count for the card; starts a request when missing.
    Q_INVOKABLE QVariantMap linkSummary(const QString &host, const QString &conversation);
    Q_INVOKABLE void refreshBlockCounts();
    // A sender's picture, {state, url, source, label}; asks the engine when
    // it is not known yet. Nothing is asked in synthetic profiles or while
    // sender pictures are off. `kind` is people, automated or unknown.
    Q_INVOKABLE QVariantMap senderPicture(const QString &email, const QString &kind);
    // The prototype's plain-text label: no scheme or `www.`.
    Q_INVOKABLE QString linkLabel(const QString &url) const;
    Q_INVOKABLE QString hostOf(const QString &url) const;
    // Blocked links in a prepared plain-text section read as disabled.
    Q_INVOKABLE QString styleLinks(const QString &html) const;

    // Returns an error to show, or "". The password is saved separately.
    Q_INVOKABLE QString saveProxy(bool enabled, const QString &hostPort, const QString &username);
    Q_INVOKABLE void setProxyPassword(const QString &password);
    Q_INVOKABLE void clearProxyPassword();
    Q_INVOKABLE void refreshProxy();
    Q_INVOKABLE void testProxy();
    Q_INVOKABLE void refreshImageCache();
    Q_INVOKABLE void clearImageCache();
signals:
    void htmlReady(const QString &ticket, const QVariantMap &result, bool ok);
    void imageReady(const QString &ticket, const QVariantMap &result, bool ok);
    void blockedHostsChanged();
    void blockCountsChanged();
    void proxyChanged();
    void imageCacheChanged();
    void revisionChanged();
    void pictureRevisionChanged();
private:
    struct ImageJob { QString ticket, account, id, version; int resource = 0; bool cachedOnly = false; };
    using Callback = std::function<void(const QJsonObject &, bool)>;
    void call(const QString &account, const QString &method, const QJsonObject &params, Callback callback);
    void pumpImages();
    void pumpPictures();
    QByteArray pictureSettings() const;
    void bump();
    QVariantMap preference(const QString &key) const;
    QList<QVariantMap> blockedList() const;
    MailBackend *m_backend;
    quint64 m_serial = 0;
    int m_revision = 0, m_pictureRevision = 0, m_picturesRunning = 0;
    quint64 m_pictureGeneration = 0;
    QByteArray m_pictureSettings;
    QHash<QString, QVariantMap> m_pictures;
    QQueue<QString> m_pictureQueue;
    QTimer m_pictureBump;
    QQueue<ImageJob> m_images;
    QHash<QString, QPair<GoogleOperation *, ImageJob>> m_running;
    QList<GoogleOperation *> m_operations;
    QHash<QString, QVariantMap> m_htmlCache;
    QList<QString> m_htmlOrder;
    QHash<QString, QVariantMap> m_summaries;
    QVariantMap m_blockCounts, m_imageCache;
    bool m_passwordSaved = false, m_testing = false;
    QVariantMap m_test;
};
