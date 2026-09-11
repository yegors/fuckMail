#pragma once

#include <QObject>
#include <QJsonObject>
#include <QLocalSocket>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>
#include <QSet>
#include <QQueue>
#include <functional>

// Phase 0: Mail UI/Calendar contract revision 5; shared fixture revision 1.
// Presentation and asynchronous IPC only. No direct database or provider access.
class MailBackend final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList accounts READ accounts NOTIFY accountsChanged)
    Q_PROPERTY(QVariantList conversations READ conversations NOTIFY conversationsChanged)
    Q_PROPERTY(QVariantList categories READ categories NOTIFY accountsChanged)
    Q_PROPERTY(QVariantMap conversation READ conversation NOTIFY conversationChanged)
    Q_PROPERTY(QVariantMap draft READ draft NOTIFY draftChanged)
    Q_PROPERTY(QVariantList events READ events NOTIFY eventsChanged)
    Q_PROPERTY(QVariantList todayEvents READ todayEvents NOTIFY eventsChanged)
    Q_PROPERTY(QString account READ account NOTIFY accountChanged)
    Q_PROPERTY(QVariantMap currentAccount READ currentAccount NOTIFY accountsChanged)
    Q_PROPERTY(QString query READ query WRITE setQuery NOTIFY queryChanged)
    Q_PROPERTY(QString folder READ folder WRITE setFolder NOTIFY queryChanged)
    Q_PROPERTY(QString category READ category WRITE setCategory NOTIFY queryChanged)
    Q_PROPERTY(QString selectedId READ selectedId NOTIFY conversationChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(QString draftStatus READ draftStatus NOTIFY draftChanged)
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY loadingChanged)
    Q_PROPERTY(bool hasMore READ hasMore NOTIFY conversationsChanged)
    Q_PROPERTY(QString weekStart READ weekStart NOTIFY eventsChanged)
    Q_PROPERTY(QString fixtureToday READ fixtureToday NOTIFY eventsChanged)
    Q_PROPERTY(QVariantMap presentation READ presentation NOTIFY presentationChanged)
public:
    explicit MailBackend(QString profile, QString engine, QObject *parent = nullptr);
    QVariantList accounts() const { return m_accounts; }
    QVariantList conversations() const { return m_conversations; }
    QVariantList categories() const { return m_categories; }
    QVariantMap conversation() const { return m_conversation; }
    QVariantMap draft() const { return m_draft; }
    QVariantList events() const { return m_events; }
    QVariantList todayEvents() const { return m_todayEvents; }
    QVariantMap currentAccount() const;
    QString account() const { return m_account; }
    QString query() const { return m_query; }
    QString folder() const { return m_folder; }
    QString category() const { return m_category; }
    QString selectedId() const { return m_selected; }
    QString error() const { return m_error; }
    QString draftStatus() const { return m_draftStatus; }
    bool connected() const { return m_connected; }
    bool loading() const { return m_loading; }
    bool hasMore() const { return m_nextOffset >= 0; }
    QString weekStart() const { return m_weekStart; }
    QString fixtureToday() const { return m_fixtureToday; }
    QVariantMap presentation() const { return m_views.value(m_account).toMap(); }
    void setQuery(const QString &value);
    void setFolder(const QString &value);
    void setCategory(const QString &value);
    Q_INVOKABLE void start();
    Q_INVOKABLE void selectAccount(const QString &id);
    Q_INVOKABLE void selectConversation(const QString &id);
    Q_INVOKABLE void loadMore();
    Q_INVOKABLE void refresh();
    Q_INVOKABLE void setRead(bool read);
    Q_INVOKABLE void setStarred(bool starred);
    Q_INVOKABLE void editDraft(const QString &body);
    Q_INVOKABLE void saveDraft();
    Q_INVOKABLE void openAttachment(const QString &id);
    Q_INVOKABLE void openLink(const QString &url);
    Q_INVOKABLE QStringList linksInText(const QString &text) const;
    Q_INVOKABLE void clearError();
    Q_INVOKABLE void moveWeek(int delta);
    Q_INVOKABLE void showFixtureWeek();
    Q_INVOKABLE void remember(const QString &key, const QVariant &value);
    Q_INVOKABLE void shutdown();
signals:
    void accountsChanged();
    void conversationsChanged();
    void loadingChanged();
    void conversationChanged();
    void draftChanged();
    void eventsChanged();
    void accountChanged();
    void queryChanged();
    void errorChanged();
    void connectedChanged();
    void presentationChanged();
    void ready();
    void shutdownReady();
private:
    using Callback = std::function<void(const QJsonObject &, bool)>;
    struct Pending { Callback callback; qint64 started; };
    void request(const QString &method, const QString &account, const QJsonObject &params, Callback callback);
    void attemptConnection();
    void sendNext();
    void receive();
    void fail(const QString &message);
    void loadAccounts();
    void loadCategories();
    void loadConversations(bool append = false, bool preserveLoaded = false);
    void loadConversationPage(const QString &account, int generation, QJsonObject params, QVariantList collected, int desiredCount);
    void loadConversation(bool loadDraft);
    void loadCalendar();
    void flag(const QString &name, bool value);
    void saveView(const QString &account);
    void finishShutdown();
    QJsonObject mutation() const;
    QString m_profile, m_engine, m_token, m_account, m_query, m_selected, m_error, m_category;
    QString m_folder = "inbox", m_weekStart = "2026-09-07", m_fixtureToday = "2026-09-11";
    QString m_draftStatus;
    QVariantList m_accounts, m_conversations, m_events, m_todayEvents, m_categories;
    QVariantMap m_conversation, m_draft, m_views;
    QHash<QString, qint64> m_viewRevisions;
    QHash<QString, Pending> m_pending;
    QQueue<QPair<QString, QByteArray>> m_outgoing;
    QString m_inFlight;
    QLocalSocket m_socket;
    QTimer m_connectTimer, m_pollTimer, m_searchTimer, m_draftTimer, m_viewTimer;
    QByteArray m_buffer;
    qint64 m_sequence = 0, m_revision = -1, m_connectStarted = 0, m_enginePid = 0;
    int m_generation = 0, m_listGeneration = 0, m_nextOffset = -1;
    bool m_connected = false, m_loading = false, m_launched = false;
    bool m_draftDirty = false, m_savingDraft = false, m_shutdown = false;
    std::function<void()> m_afterDraftSave;
    QSet<QString> m_viewSaving, m_viewDirty;
};
