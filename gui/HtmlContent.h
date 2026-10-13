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

/// The HTML reader's engine calls: sanitised message HTML, image fetches, link
/// counts, the blocklist and the HTTPS proxy settings. Each call uses its own
/// connection, so a slow image never holds the mail pipe.
///
/// MailBackend creates one lazily (its `htmlContent` property) as a child
/// QObject; QML uses it from the reader, sender avatars and the remote
/// content settings. GUI thread only. The blocklist, per-sender image
/// permissions and proxy host settings live in MailBackend's preferences;
/// the proxy password is handed to the engine and never kept here.
///
/// Results that depend on the engine's store are cached per connection and
/// dropped when the engine disconnects.
class HtmlContent final : public QObject {
    Q_OBJECT
    /// Blocked patterns, newest first: [{pattern, source, added_at}].
    Q_PROPERTY(QVariantList blockedHosts READ blockedHosts NOTIFY blockedHostsChanged)
    /// pattern → messages linking to it, plus `complete` while the index catches up.
    /// Filled by refreshBlockCounts().
    Q_PROPERTY(QVariantMap blockCounts READ blockCounts NOTIFY blockCountsChanged)
    /// {enabled, host, port, username, password_saved, testing, test}.
    /// `test` holds the last testProxy() result, {ok, reason, ...}.
    Q_PROPERTY(QVariantMap proxy READ proxy NOTIFY proxyChanged)
    /// {bytes, limit} of the on-disk image cache.
    Q_PROPERTY(QVariantMap imageCache READ imageCache NOTIFY imageCacheChanged)
    /// Changes whenever link summaries or the blocklist change. QML bindings
    /// read it so that linkSummary() and blockedBy() are evaluated again.
    Q_PROPERTY(int revision READ revision NOTIFY revisionChanged)
    /// Changes when sender pictures arrive, at most once per 40 ms batch.
    /// Bindings read it so that senderPicture() is evaluated again.
    Q_PROPERTY(int pictureRevision READ pictureRevision NOTIFY pictureRevisionChanged)
public:
    explicit HtmlContent(MailBackend *backend);
    /// Cancels outstanding engine calls without delivering their results.
    ~HtmlContent() override;
    QVariantList blockedHosts() const;
    QVariantMap blockCounts() const { return m_blockCounts; }
    QVariantMap proxy() const;
    QVariantMap imageCache() const { return m_imageCache; }
    int revision() const { return m_revision; }
    int pictureRevision() const { return m_pictureRevision; }

    /// Returns a ticket; `htmlReady(ticket, result, ok)` follows.
    /// The eight most recent successful results, keyed by account, id and
    /// `sourceVersion`, are answered from memory. `quotes` asks for the view
    /// with quoted reply history kept, a separate result with its own image ids.
    Q_INVOKABLE QString load(const QString &account, const QString &id, const QString &sourceVersion, bool quotes = false);
    /// A result already held, with the images loaded since; empty otherwise.
    /// Never contacts the engine.
    Q_INVOKABLE QVariantMap cached(const QString &account, const QString &id, const QString &sourceVersion, bool quotes = false);
    /// Returns a ticket; `imageReady(ticket, result, ok)` follows.
    /// `resource` is the N of an `fm-res:N` reference in the sanitised HTML.
    /// Jobs queue and run at most six at a time. `cachedOnly` asks for the
    /// engine's on-disk copy without a network fetch. `quotes` names an image
    /// of the view load() returned with the same flag.
    Q_INVOKABLE QString loadImage(const QString &account, const QString &id, const QString &sourceVersion,
                                  int resource, bool cachedOnly = false, bool quotes = false);
    /// Drops queued and running image work for one message.
    /// Dropped tickets get no imageReady signal.
    Q_INVOKABLE void cancelMessage(const QString &account, const QString &id);

    /// Normalises a typed or pasted domain, or `*.domain`, to a blocklist
    /// pattern (lowercase ASCII, no scheme, path or `www.`); "" when invalid.
    Q_INVOKABLE QString normalizePattern(const QString &input) const;
    /// The first blocklist pattern that matches `host`, or "".
    Q_INVOKABLE QString blockedBy(const QString &host) const;
    /// True when the host of URL `href` is blocked.
    Q_INVOKABLE bool linkBlocked(const QString &href) const;
    /// Adds a pattern; returns an error to show, or "".
    /// `source` is "link" when blocked from a link card, else "settings".
    /// At most 1,000 patterns. Saves preferences.
    Q_INVOKABLE QString block(const QString &input, const QString &source);
    /// Removes an exact stored pattern. Saves preferences when it was present.
    Q_INVOKABLE void unblock(const QString &pattern);
    /// True when the user chose to load remote images from sender `email`
    /// in `account`. Addresses compare case-insensitively.
    Q_INVOKABLE bool imagesAllowed(const QString &account, const QString &email) const;
    /// Remembers that remote images from `email` load automatically in
    /// `account`. Saves preferences.
    Q_INVOKABLE void allowImages(const QString &account, const QString &email);
    /// Forgets an allowImages() choice. Saves preferences when it existed.
    Q_INVOKABLE void disallowImages(const QString &account, const QString &email);

    /// Cached count for the card; starts a request when missing.
    /// Returns {loading, ...} until the engine answers, then bumps `revision`.
    /// An incomplete count is asked again at most every five seconds.
    Q_INVOKABLE QVariantMap linkSummary(const QString &host, const QString &conversation);
    /// Asks the engine how many stored messages link to each blocked pattern;
    /// the answer lands in `blockCounts`. Failures leave it unchanged.
    Q_INVOKABLE void refreshBlockCounts();
    /// A sender's picture, {state, url, source, label}; asks the engine when
    /// it is not known yet. Nothing is asked in synthetic profiles or while
    /// sender pictures are off. `kind` is people, automated or unknown.
    /// Failed or transient lookups are retried after a minute; at most four
    /// run at once.
    Q_INVOKABLE QVariantMap senderPicture(const QString &email, const QString &kind);
    /// The design's plain-text label: no scheme or `www.`.
    Q_INVOKABLE QString linkLabel(const QString &url) const;
    /// The URL's host, lowercase and without `www.`; "" for an invalid URL.
    Q_INVOKABLE QString hostOf(const QString &url) const;
    /// Blocked links in a prepared plain-text section read as disabled.
    /// Rewrites only anchors in the exact `<a href="...">` form that
    /// ReceivedBody and ReplyDocument::limitedHtml() emit.
    Q_INVOKABLE QString styleLinks(const QString &html) const;

    /// Returns an error to show, or "". The password is saved separately.
    /// `hostPort` is `host` or `host:port` (default port 443). Saves
    /// preferences and clears the last test result.
    Q_INVOKABLE QString saveProxy(bool enabled, const QString &hostPort, const QString &username);
    /// Sends the proxy password to the engine, which keeps it in the OS
    /// credential store. An empty password is ignored.
    Q_INVOKABLE void setProxyPassword(const QString &password);
    /// Asks the engine to delete the stored proxy password.
    Q_INVOKABLE void clearProxyPassword();
    /// Refreshes `proxy.password_saved` from the engine.
    Q_INVOKABLE void refreshProxy();
    /// Asks the engine to fetch a fixed test URL through the saved proxy.
    /// `proxy.testing` is true meanwhile; the outcome lands in `proxy.test`.
    /// Ignored while a test is already running.
    Q_INVOKABLE void testProxy();
    /// Refreshes `imageCache` from the engine.
    Q_INVOKABLE void refreshImageCache();
    /// Deletes the engine's cached images and sender pictures, and drops the
    /// in-memory HTML and picture caches that point at those files.
    Q_INVOKABLE void clearImageCache();
signals:
    /// Answers load(). Always emitted from the event loop, after load() has
    /// returned `ticket`. `result` is the engine's `message_html` result, or
    /// {code, message} when `ok` is false.
    void htmlReady(const QString &ticket, const QVariantMap &result, bool ok);
    /// Answers loadImage(), from the event loop. A ready image has
    /// {state: "ready", path, width, height}, where `path` is a local file in
    /// the engine's cache; other states include "blocked" and "not_cached".
    void imageReady(const QString &ticket, const QVariantMap &result, bool ok);
    void blockedHostsChanged();
    void blockCountsChanged();
    void proxyChanged();
    void imageCacheChanged();
    void revisionChanged();
    void pictureRevisionChanged();
private:
    struct ImageJob { QString ticket, account, id, version; int resource = 0; bool cachedOnly = false, quotes = false; };
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
