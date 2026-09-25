#include "HtmlContent.h"
#include "GoogleOperation.h"
#include "HtmlLinks.h"
#include "MailBackend.h"
#include <QDateTime>
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QUrl>
#include <algorithm>

namespace {
constexpr int maxImageOperations = 6;
constexpr int htmlCacheSize = 8;
constexpr int maxBlockedHosts = 1000;
constexpr qint64 summaryRetryMs = 5000;
constexpr int maxPictureOperations = 4;
// A picture that failed to arrive (not one found missing) is asked again later.
constexpr qint64 pictureRetryMs = 60 * 1000;
}

// MailBackend owns one instance, created when QML first asks for it.
QObject *MailBackend::htmlContent() const {
    if (!m_htmlContent) m_htmlContent = new HtmlContent(const_cast<MailBackend *>(this));
    return m_htmlContent;
}

HtmlContent::HtmlContent(MailBackend *backend) : QObject(backend), m_backend(backend) {
    m_pictureSettings = pictureSettings();
    // Pictures arriving together repaint the list once.
    m_pictureBump.setSingleShot(true); m_pictureBump.setInterval(40);
    connect(&m_pictureBump, &QTimer::timeout, this, [this] { ++m_pictureRevision; emit pictureRevisionChanged(); });
    connect(backend, &MailBackend::preferencesChanged, this, [this] {
        emit blockedHostsChanged(); emit proxyChanged(); bump();
        // Turning pictures on or off, blocking a host or changing the proxy
        // can change what is found.
        if (auto settings = pictureSettings(); settings != m_pictureSettings) {
            m_pictureSettings = settings;
            m_pictures.clear(); m_pictureQueue.clear(); ++m_pictureGeneration; m_pictureBump.start();
        }
    });
    connect(backend, &MailBackend::accountChanged, this, [this] { bump(); });
    connect(backend, &MailBackend::connectedChanged, this, [this] {
        if (m_backend->connected()) return;
        // A new engine may have a different store; ask again after reconnecting.
        m_summaries.clear(); m_htmlCache.clear(); m_htmlOrder.clear();
        m_pictures.clear(); m_pictureQueue.clear(); ++m_pictureGeneration;
        bump(); m_pictureBump.start();
    });
}

HtmlContent::~HtmlContent() {
    for (auto &running : m_running) if (running.first) running.first->cancel(false);
    for (auto operation : std::as_const(m_operations)) operation->cancel(false);
}

void HtmlContent::bump() { ++m_revision; emit revisionChanged(); }

void HtmlContent::call(const QString &account, const QString &method, const QJsonObject &params, Callback callback) {
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

QString HtmlContent::load(const QString &account, const QString &id, const QString &sourceVersion) {
    const auto ticket = QStringLiteral("html-%1").arg(++m_serial);
    const auto key = account + '/' + id + '/' + sourceVersion;
    if (const auto cached = m_htmlCache.constFind(key); cached != m_htmlCache.cend()) {
        m_htmlOrder.removeOne(key); m_htmlOrder.append(key);
        QMetaObject::invokeMethod(this, [this, ticket, result = *cached] { emit htmlReady(ticket, result, true); }, Qt::QueuedConnection);
        return ticket;
    }
    call(account, "message_html", {{"id", id}, {"source_version", sourceVersion}}, [this, ticket, key](const QJsonObject &result, bool ok) {
        const auto value = result.toVariantMap();
        if (ok) {
            m_htmlCache.insert(key, value); m_htmlOrder.removeOne(key); m_htmlOrder.append(key);
            while (m_htmlOrder.size() > htmlCacheSize) m_htmlCache.remove(m_htmlOrder.takeFirst());
        }
        emit htmlReady(ticket, value, ok);
    });
    return ticket;
}

QVariantMap HtmlContent::cached(const QString &account, const QString &id, const QString &sourceVersion) {
    const auto key = account + '/' + id + '/' + sourceVersion;
    const auto hit = m_htmlCache.constFind(key);
    if (hit == m_htmlCache.cend()) return {};
    m_htmlOrder.removeOne(key); m_htmlOrder.append(key);
    return *hit;
}

QString HtmlContent::loadImage(const QString &account, const QString &id, const QString &sourceVersion, int resource, bool cachedOnly) {
    ImageJob job{QStringLiteral("image-%1").arg(++m_serial), account, id, sourceVersion, resource, cachedOnly};
    m_images.enqueue(job);
    QMetaObject::invokeMethod(this, &HtmlContent::pumpImages, Qt::QueuedConnection);
    return job.ticket;
}

void HtmlContent::pumpImages() {
    while (m_running.size() < maxImageOperations && !m_images.isEmpty()) {
        const auto job = m_images.dequeue();
        if (!m_backend->m_connected || m_backend->m_enginePid <= 0) {
            emit imageReady(job.ticket, {{"code", "engine_disconnected"}, {"message", "The local engine is not connected."}}, false);
            continue;
        }
        QJsonObject params{{"id", job.id}, {"source_version", job.version}, {"resource", job.resource}};
        if (job.cachedOnly) params["cached_only"] = true;
        auto operation = new GoogleOperation(job.account, {}, this);
        m_running.insert(job.ticket, {operation, job});
        operation->start(m_backend->m_profile, m_backend->m_enginePid, "message_image", params, [this, ticket = job.ticket](auto result, bool ok) {
            const auto running = m_running.take(ticket);
            if (running.first) running.first->deleteLater();
            const auto value = result.toVariantMap();
            // Reopening from the GUI cache shows what already loaded.
            const auto &job = running.second;
            const auto key = job.account + '/' + job.id + '/' + job.version;
            if (ok && value.value("state") == "ready" && m_htmlCache.contains(key)) {
                auto cached = m_htmlCache.value(key);
                auto resources = cached.value("resources").toList();
                for (auto &entry : resources) {
                    auto resource = entry.toMap();
                    if (resource.value("id").toInt() != job.resource) continue;
                    resource["image"] = QVariantMap{{"path", value.value("path")}, {"width", value.value("width")}, {"height", value.value("height")}};
                    entry = resource;
                }
                cached["resources"] = resources;
                m_htmlCache.insert(key, cached);
            }
            QMetaObject::invokeMethod(this, [this, ticket, value, ok] { emit imageReady(ticket, value, ok); pumpImages(); }, Qt::QueuedConnection);
        });
    }
}

void HtmlContent::cancelMessage(const QString &account, const QString &id) {
    QQueue<ImageJob> kept;
    for (const auto &job : std::as_const(m_images)) if (job.account != account || job.id != id) kept.enqueue(job);
    m_images.swap(kept);
    for (auto it = m_running.begin(); it != m_running.end();) {
        if (it->second.account == account && it->second.id == id) {
            // Closing the connection cancels the engine's fetch.
            auto operation = it->first;
            it = m_running.erase(it);
            operation->cancel(false);
            operation->deleteLater();
        } else ++it;
    }
    QMetaObject::invokeMethod(this, &HtmlContent::pumpImages, Qt::QueuedConnection);
}

QVariantMap HtmlContent::preference(const QString &key) const { return m_backend->preferences().value(key).toMap(); }

QList<QVariantMap> HtmlContent::blockedList() const {
    QList<QVariantMap> list;
    for (const auto &value : m_backend->preferences().value("blocked_hosts").toList()) list.append(value.toMap());
    return list;
}

QVariantList HtmlContent::blockedHosts() const {
    auto list = blockedList();
    std::stable_sort(list.begin(), list.end(), [](const QVariantMap &a, const QVariantMap &b) {
        return a.value("added_at").toLongLong() > b.value("added_at").toLongLong();
    });
    QVariantList result;
    for (const auto &entry : std::as_const(list)) result.append(entry);
    return result;
}

QString HtmlContent::normalizePattern(const QString &input) const { return HtmlLinks::normalizePattern(input); }

QString HtmlContent::blockedBy(const QString &host) const {
    QStringList patterns;
    for (const auto &entry : blockedList()) patterns.append(entry.value("pattern").toString());
    return HtmlLinks::blockedBy(patterns, host);
}

QString HtmlContent::hostOf(const QString &url) const { return HtmlLinks::hostOf(url); }

bool HtmlContent::linkBlocked(const QString &href) const {
    const auto host = hostOf(href);
    return !host.isEmpty() && !blockedBy(host).isEmpty();
}

QString HtmlContent::block(const QString &input, const QString &source) {
    const auto pattern = normalizePattern(input);
    if (pattern.isEmpty()) return QStringLiteral("Use a domain like tracker.io");
    auto list = m_backend->preferences().value("blocked_hosts").toList();
    for (const auto &entry : std::as_const(list)) if (entry.toMap().value("pattern") == pattern) return QStringLiteral("Already blocked");
    if (list.size() >= maxBlockedHosts) return QStringLiteral("At most 1,000 domains can be blocked");
    list.prepend(QVariantMap{{"pattern", pattern}, {"source", source == "link" ? "link" : "settings"},
                             {"added_at", QDateTime::currentSecsSinceEpoch()}});
    m_backend->setPreferences({{"blocked_hosts", list}});
    return {};
}

void HtmlContent::unblock(const QString &pattern) {
    auto list = m_backend->preferences().value("blocked_hosts").toList();
    const auto before = list.size();
    list.removeIf([&](const QVariant &entry) { return entry.toMap().value("pattern") == pattern; });
    if (list.size() != before) m_backend->setPreferences({{"blocked_hosts", list}});
}

bool HtmlContent::imagesAllowed(const QString &account, const QString &email) const {
    const auto address = email.trimmed().toLower();
    for (const auto &value : m_backend->preferences().value("remote_images_from").toList()) {
        const auto entry = value.toMap();
        if (entry.value("account") == account && entry.value("email") == address) return true;
    }
    return false;
}

void HtmlContent::allowImages(const QString &account, const QString &email) {
    const auto address = email.trimmed().toLower();
    if (account.isEmpty() || !address.contains('@') || imagesAllowed(account, address)) return;
    auto list = m_backend->preferences().value("remote_images_from").toList();
    list.append(QVariantMap{{"account", account}, {"email", address}});
    m_backend->setPreferences({{"remote_images_from", list}});
}

void HtmlContent::disallowImages(const QString &account, const QString &email) {
    const auto address = email.trimmed().toLower();
    auto list = m_backend->preferences().value("remote_images_from").toList();
    const auto before = list.size();
    list.removeIf([&](const QVariant &value) {
        const auto entry = value.toMap();
        return entry.value("account") == account && entry.value("email") == address;
    });
    if (list.size() != before) m_backend->setPreferences({{"remote_images_from", list}});
}

QVariantMap HtmlContent::linkSummary(const QString &input, const QString &conversation) {
    const auto account = m_backend->account();
    const auto host = HtmlLinks::withoutWww(input);
    if (host.isEmpty() || account.isEmpty()) return {};
    const auto key = account + '|' + host + '|' + conversation;
    const auto found = m_summaries.constFind(key);
    const auto now = QDateTime::currentMSecsSinceEpoch();
    if (found != m_summaries.cend()) {
        // An incomplete count is asked again, at most every few seconds.
        const bool retry = !found->value("loading").toBool() && !found->value("complete").toBool()
            && now - found->value("at").toLongLong() > summaryRetryMs;
        if (!retry) return *found;
    }
    auto pending = found != m_summaries.cend() ? *found : QVariantMap{};
    pending["loading"] = true;
    m_summaries.insert(key, pending);
    QJsonObject params{{"host", host}};
    if (!conversation.isEmpty()) params["conversation"] = conversation;
    call(account, "link_host_summary", params, [this, key](const QJsonObject &result, bool ok) {
        auto value = ok ? result.toVariantMap() : QVariantMap{{"error", result["message"].toString()}};
        value["at"] = QDateTime::currentMSecsSinceEpoch();
        m_summaries.insert(key, value);
        bump();
    });
    return pending;
}

QByteArray HtmlContent::pictureSettings() const {
    const auto preferences = m_backend->preferences();
    return QJsonDocument(QJsonObject{{"on", preferences.value("sender_pictures", true).toBool()},
        {"blocked", QJsonValue::fromVariant(preferences.value("blocked_hosts"))},
        {"proxy", QJsonValue::fromVariant(preferences.value("remote_proxy"))}}).toJson(QJsonDocument::Compact);
}

QVariantMap HtmlContent::senderPicture(const QString &email, const QString &kind) {
    static const QVariantMap none{{"state", "none"}};
    const auto address = email.trimmed().toLower();
    if (!m_backend->preferences().value("sender_pictures", true).toBool() || m_backend->syntheticProfile() || !address.contains('@')) return none;
    const auto key = kind + '|' + address;
    const auto found = m_pictures.constFind(key);
    if (found != m_pictures.cend()) {
        const bool retry = (found->value("state") == "error" || found->value("transient").toBool())
            && QDateTime::currentMSecsSinceEpoch() - found->value("at").toLongLong() > pictureRetryMs;
        if (!retry) return *found;
    }
    m_pictures.insert(key, {{"state", "loading"}});
    m_pictureQueue.enqueue(key);
    QMetaObject::invokeMethod(this, &HtmlContent::pumpPictures, Qt::QueuedConnection);
    return m_pictures.value(key);
}

void HtmlContent::pumpPictures() {
    while (m_picturesRunning < maxPictureOperations && !m_pictureQueue.isEmpty()) {
        const auto key = m_pictureQueue.dequeue();
        const auto split = key.indexOf('|');
        ++m_picturesRunning;
        call({}, "sender_avatar", {{"address", key.mid(split + 1)}, {"kind", key.left(split)}}, [this, key, generation = m_pictureGeneration](const QJsonObject &result, bool ok) {
            --m_picturesRunning;
            // Settings or the engine changed while this was asked; its answer is stale.
            if (generation == m_pictureGeneration) {
                auto value = ok ? result.toVariantMap() : QVariantMap{{"state", "error"}};
                if (value.value("state") == "ready") value["url"] = QUrl::fromLocalFile(value.value("path").toString()).toString();
                value["at"] = QDateTime::currentMSecsSinceEpoch();
                m_pictures.insert(key, value);
                m_pictureBump.start();
            }
            pumpPictures();
        });
    }
}

void HtmlContent::refreshBlockCounts() {
    QJsonArray patterns;
    for (const auto &entry : blockedList()) patterns.append(entry.value("pattern").toString());
    call({}, "link_pattern_counts", {{"patterns", patterns}}, [this](const QJsonObject &result, bool ok) {
        if (!ok) return;
        m_blockCounts = result.toVariantMap();
        emit blockCountsChanged();
    });
}

QString HtmlContent::linkLabel(const QString &url) const { return HtmlLinks::label(url); }

QString HtmlContent::styleLinks(const QString &html) const {
    if (blockedList().isEmpty()) return html;
    static const QRegularExpression anchor(QStringLiteral("<a href=\"([^\"]*)\">"));
    QString result;
    qsizetype end = 0;
    auto matches = anchor.globalMatch(html);
    while (matches.hasNext()) {
        const auto match = matches.next();
        result += html.mid(end, match.capturedStart() - end);
        const auto href = match.captured(1);
        QString decoded = href; decoded.replace("&amp;", "&").replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">");
        result += linkBlocked(decoded) ? "<a href=\"" + href + "\" style=\"color:#8a8f98;text-decoration:line-through\">" : match.captured();
        end = match.capturedEnd();
    }
    return result + html.mid(end);
}

QVariantMap HtmlContent::proxy() const {
    auto value = preference("remote_proxy");
    value["password_saved"] = m_passwordSaved;
    value["testing"] = m_testing;
    value["test"] = m_test;
    return value;
}

QString HtmlContent::saveProxy(bool enabled, const QString &hostPort, const QString &username) {
    auto value = hostPort.trimmed().toLower();
    int port = 443;
    static const QRegularExpression withPort(QStringLiteral("^(.*):(\\d{1,5})$"));
    if (const auto match = withPort.match(value); match.hasMatch()) { value = match.captured(1); port = match.captured(2).toInt(); }
    if (value.contains(QLatin1String("://")) || value.contains('/') || value.contains('@') || value.contains(' ') || value.contains(':'))
        return QStringLiteral("Enter the proxy's host name, without https:// or a path");
    if (port < 1 || port > 65535) return QStringLiteral("Use a port from 1 to 65535");
    if (enabled && value.isEmpty()) return QStringLiteral("Enter the proxy's host name");
    m_test.clear();
    m_backend->setPreferences({{"remote_proxy", QVariantMap{{"enabled", enabled}, {"host", value}, {"port", port}, {"username", username.trimmed()}}}});
    emit proxyChanged();
    return {};
}

void HtmlContent::setProxyPassword(const QString &password) {
    if (password.isEmpty()) return;
    call({}, "remote_proxy_password", {{"op", "set"}, {"password", password}}, [this](const QJsonObject &result, bool ok) {
        if (ok) m_passwordSaved = result["saved"].toBool();
        m_test.clear();
        if (!ok) m_test = {{"ok", false}, {"reason", result["message"].toString("The password could not be saved.")}};
        emit proxyChanged();
    });
}

void HtmlContent::clearProxyPassword() {
    call({}, "remote_proxy_password", {{"op", "clear"}}, [this](const QJsonObject &result, bool ok) {
        if (ok) m_passwordSaved = result["saved"].toBool();
        emit proxyChanged();
    });
}

void HtmlContent::refreshProxy() {
    call({}, "remote_proxy_password", {{"op", "status"}}, [this](const QJsonObject &result, bool ok) {
        if (ok) { m_passwordSaved = result["saved"].toBool(); emit proxyChanged(); }
    });
}

void HtmlContent::testProxy() {
    if (m_testing) return;
    m_testing = true; m_test.clear();
    emit proxyChanged();
    call({}, "remote_proxy_test", {}, [this](const QJsonObject &result, bool ok) {
        m_testing = false;
        m_test = ok ? result.toVariantMap() : QVariantMap{{"ok", false}, {"reason", result["message"].toString("The test could not run.")}};
        emit proxyChanged();
    });
}

void HtmlContent::refreshImageCache() {
    call({}, "remote_image_cache", {{"op", "stats"}}, [this](const QJsonObject &result, bool ok) {
        if (ok) { m_imageCache = result.toVariantMap(); emit imageCacheChanged(); }
    });
}

void HtmlContent::clearImageCache() {
    call({}, "remote_image_cache", {{"op", "clear"}}, [this](const QJsonObject &result, bool ok) {
        if (ok) { m_imageCache = result.toVariantMap(); emit imageCacheChanged(); }
        m_htmlCache.clear(); m_htmlOrder.clear();
        // Sender pictures are cleared with the images; their files are gone.
        m_pictures.clear(); m_pictureQueue.clear(); ++m_pictureGeneration; m_pictureBump.start();
    });
}
