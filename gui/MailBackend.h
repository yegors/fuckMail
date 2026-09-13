#pragma once

#include <QObject>
#include <QElapsedTimer>
#include <QJsonObject>
#include <QLocalSocket>
#include <QTimer>
#include <QDateTime>
#include <QVariantList>
#include <QVariantMap>
#include <QSet>
#include <QQueue>
#include <functional>

class GoogleOperation;

// Phase 1: shared Core/Mail/Calendar boundary, additive protocol 1.
// Presentation and asynchronous IPC only. No direct database or provider access.
class MailBackend final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList accounts READ accounts NOTIFY accountsChanged)
    Q_PROPERTY(QVariantList conversations READ conversations NOTIFY conversationsChanged)
    Q_PROPERTY(QVariantList categories READ categories NOTIFY categoriesChanged)
    Q_PROPERTY(QVariantMap conversation READ conversation NOTIFY conversationChanged)
    Q_PROPERTY(QVariantMap senderCard READ senderCard NOTIFY senderCardChanged)
    Q_PROPERTY(QVariantMap draft READ draft NOTIFY draftChanged)
    Q_PROPERTY(QString activeDraftId READ activeDraftId NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap recipientInputs READ recipientInputs NOTIFY draftChanged)
    Q_PROPERTY(QVariantList events READ events NOTIFY eventsChanged)
    Q_PROPERTY(QVariantList todayEvents READ todayEvents NOTIFY eventsChanged)
    Q_PROPERTY(QString account READ account NOTIFY accountChanged)
    Q_PROPERTY(QVariantMap currentAccount READ currentAccount NOTIFY currentAccountChanged)
    Q_PROPERTY(QString query READ query WRITE setQuery NOTIFY queryChanged)
    Q_PROPERTY(QString folder READ folder WRITE setFolder NOTIFY queryChanged)
    Q_PROPERTY(QString category READ category WRITE setCategory NOTIFY queryChanged)
    Q_PROPERTY(QString selectedId READ selectedId NOTIFY conversationChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(QString draftStatus READ draftStatus NOTIFY draftChanged)
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    Q_PROPERTY(bool syntheticProfile READ syntheticProfile NOTIFY connectedChanged)
    Q_PROPERTY(QVariantMap googleAuth READ googleAuth NOTIFY googleAuthChanged)
    Q_PROPERTY(bool googleAuthBusy READ googleAuthBusy NOTIFY googleAuthChanged)
    Q_PROPERTY(QVariantMap googleSync READ googleSync NOTIFY googleSyncChanged)
    Q_PROPERTY(QVariantMap googleSyncLimits READ googleSyncLimits NOTIFY googleSyncChanged)
    Q_PROPERTY(QVariantMap mailDelivery READ mailDelivery NOTIFY mailDeliveryChanged)
    Q_PROPERTY(QVariantList googleLabels READ googleLabels NOTIFY googleLabelsChanged)
    Q_PROPERTY(QVariantMap googleLabelState READ googleLabelState NOTIFY googleLabelsChanged)
    Q_PROPERTY(bool loading READ loading NOTIFY loadingChanged)
    Q_PROPERTY(bool mutationInFlight READ mutationInFlight NOTIFY mutationChanged)
    Q_PROPERTY(bool hasMore READ hasMore NOTIFY conversationsChanged)
    Q_PROPERTY(QString weekStart READ weekStart NOTIFY eventsChanged)
    Q_PROPERTY(QString fixtureToday READ fixtureToday NOTIFY eventsChanged)
    Q_PROPERTY(QVariantMap presentation READ presentation NOTIFY presentationChanged)
    Q_PROPERTY(QVariantMap preferences READ preferences NOTIFY preferencesChanged)
    Q_PROPERTY(QString preferencesStatus READ preferencesStatus NOTIFY preferencesChanged)
    Q_PROPERTY(bool preferencesPending READ preferencesPending NOTIFY preferencesChanged)
    Q_PROPERTY(QVariantMap diagnostics READ diagnostics NOTIFY preferencesChanged)
    Q_PROPERTY(QVariantMap filters READ filters NOTIFY queryChanged)
    Q_PROPERTY(QStringList collapsedGroups READ collapsedGroups NOTIFY queryChanged)
    Q_PROPERTY(QVariantMap querySummary READ querySummary NOTIFY conversationsChanged)
    Q_PROPERTY(QVariantList captures READ captures NOTIFY capturesChanged)
    Q_PROPERTY(QString captureStatus READ captureStatus NOTIFY capturesChanged)
    Q_PROPERTY(bool capturesMore READ capturesMore NOTIFY capturesChanged)
    Q_PROPERTY(bool draftBusy READ draftBusy NOTIFY draftChanged)
    Q_PROPERTY(bool draftLocked READ draftLocked NOTIFY draftChanged)
    Q_PROPERTY(bool draftConflict READ draftConflict NOTIFY draftChanged)
    Q_PROPERTY(QVariantList calendarSources READ calendarSources NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarView READ calendarView NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarDate READ calendarDate NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarFrom READ calendarFrom NOTIFY eventsChanged)
    Q_PROPERTY(int calendarDays READ calendarDays NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarCoverage READ calendarCoverage NOTIFY eventsChanged)
    Q_PROPERTY(QString timelineCoverage READ timelineCoverage NOTIFY eventsChanged)
    Q_PROPERTY(bool calendarReady READ calendarReady NOTIFY eventsChanged)
    Q_PROPERTY(bool timelineReady READ timelineReady NOTIFY eventsChanged)
    Q_PROPERTY(QVariantList calendarOperations READ calendarOperations NOTIFY calendarOperationsChanged)
    Q_PROPERTY(bool calendarOperationsLoading READ calendarOperationsLoading NOTIFY calendarOperationsChanged)
    Q_PROPERTY(bool calendarOperationsHasMore READ calendarOperationsHasMore NOTIFY calendarOperationsChanged)
    Q_PROPERTY(QString calendarOperationStatus READ calendarOperationStatus NOTIFY calendarOperationsChanged)
    Q_PROPERTY(QString timelineDate READ timelineDate NOTIFY eventsChanged)
    Q_PROPERTY(QString todayDate READ todayDate NOTIFY eventsChanged)
    Q_PROPERTY(double nowHour READ nowHour NOTIFY eventsChanged)
    Q_PROPERTY(QDateTime clockNow READ clockNow NOTIFY eventsChanged)
public:
    explicit MailBackend(QString profile, QString engine, QObject *parent = nullptr, bool fixtureClock = false, bool liveProfile = false);
    ~MailBackend() override;
    QVariantList accounts() const { return m_accounts; }
    QVariantList conversations() const { return m_conversations; }
    QVariantList categories() const { return m_categories; }
    QVariantMap conversation() const { return m_conversation; }
    QVariantMap senderCard() const { return m_senderCard; }
    QVariantMap draft() const { return m_draft; }
    QString activeDraftId() const { return m_activeDraft; }
    QVariantMap recipientInputs() const { QVariantMap values; for (auto it = m_recipientInputs.cbegin(); it != m_recipientInputs.cend(); ++it) values[it.key()] = it.value(); return values; }
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
    bool syntheticProfile() const { return m_syntheticProfile; }
    QVariantMap googleAuth() const { return m_googleAuth; }
    bool googleAuthBusy() const { return m_googleAuthBusy; }
    QVariantMap googleSync() const { return m_googleSync; }
    QVariantMap googleSyncLimits() const { return m_googleSyncLimits; }
    QVariantMap mailDelivery() const { return m_mailDelivery.value({m_account, m_selected}); }
    QVariantList googleLabels() const { return m_googleLabels; }
    QVariantMap googleLabelState() const;
    bool loading() const { return m_loading; }
    bool mutationInFlight() const { return m_mutatingConversation; }
    bool hasMore() const { return m_nextOffset >= 0; }
    QString weekStart() const { return m_weekStart; }
    QString fixtureToday() const { return m_fixtureToday; }
    QVariantMap presentation() const { return m_views.value(m_account).toMap(); }
    QVariantMap preferences() const { return m_preferences; }
    QString preferencesStatus() const { return m_preferencesStatus; }
    bool preferencesPending() const { return !m_preferencesPending.isEmpty(); }
    QVariantMap diagnostics() const { return m_diagnostics; }
    QVariantMap filters() const { return m_filters; }
    QStringList collapsedGroups() const { return m_collapsedGroups; }
    QVariantMap querySummary() const { return m_querySummary; }
    QVariantList captures() const { return m_captures; }
    QString captureStatus() const { return m_captureStatus; }
    bool capturesMore() const { return m_captureNextOffset >= 0; }
    bool draftBusy() const { return m_savingDraft || m_capturing || m_discarding || m_resolvingDraft; }
    bool draftLocked() const { return m_capturing || m_discarding || m_resolvingDraft; }
    bool draftConflict() const { return m_draftConflict; }
    QVariantList calendarSources() const { return m_calendarSources; }
    QString calendarView() const { return m_calendarView; }
    QString calendarDate() const { return m_calendarDate; }
    QString calendarFrom() const;
    int calendarDays() const;
    QString calendarCoverage() const { return m_calendarCoverage; }
    QString timelineCoverage() const { return m_timelineCoverage; }
    bool calendarReady() const { return m_calendarReady; }
    bool timelineReady() const { return m_timelineReady; }
    QVariantList calendarOperations() const { return m_calendarOperations; }
    bool calendarOperationsLoading() const { return m_calendarOperationsLoading; }
    bool calendarOperationsHasMore() const { return !m_calendarOperationCursors.isEmpty(); }
    QString calendarOperationStatus() const { return m_calendarOperationStatus; }
    QString timelineDate() const { return m_timelineDate; }
    QString todayDate() const;
    double nowHour() const;
    QDateTime clockNow() const;
    void setQuery(const QString &value);
    void setFolder(const QString &value);
    void setCategory(const QString &value);
    Q_INVOKABLE void start();
    Q_INVOKABLE void selectAccount(const QString &id);
    Q_INVOKABLE void selectConversation(const QString &id);
    Q_INVOKABLE void openConversation(const QString &id);
    Q_INVOKABLE void requestSenderCard(const QString &email);
    Q_INVOKABLE void clearSenderCard();
    Q_INVOKABLE void writeToSender(const QString &email, const QString &name);
    Q_INVOKABLE void loadMore();
    Q_INVOKABLE void refresh();
    Q_INVOKABLE void connectGoogle(const QString &configFile, const QString &account = {}, bool reauthorize = false);
    Q_INVOKABLE void cancelGoogleAuth();
    Q_INVOKABLE void disconnectGoogle(const QString &account);
    Q_INVOKABLE bool accountCapability(const QString &account, const QString &name) const;
    Q_INVOKABLE QVariantMap googleSyncState(const QString &account) const;
    Q_INVOKABLE void startGoogleSync(const QString &account, const QVariantMap &limits);
    Q_INVOKABLE void pauseGoogleSync(const QString &account);
    Q_INVOKABLE void cancelGoogleOperation(const QString &account, const QString &requestId);
    Q_INVOKABLE void loadGoogleLabels(const QString &account, bool append = false);
    Q_INVOKABLE void refreshGoogleLabels(const QString &account);
    Q_INVOKABLE void bindGoogleLabel(const QString &account, const QString &labelId, const QString &categoryId, const QString &createName, const QVariant &expectedProviderLabelId, qint64 expectedCatalogRevision);
    Q_INVOKABLE void retryGoogleLabelBinding(const QString &account);
    Q_INVOKABLE void setRead(bool read);
    Q_INVOKABLE void setStarred(bool starred);
    Q_INVOKABLE void doneConversation();
    Q_INVOKABLE void setCategoryMembership(const QString &categoryId, bool present);
    Q_INVOKABLE void fileConversation(const QString &action);
    Q_INVOKABLE void editDraft(const QString &body);
    Q_INVOKABLE void beginDraft(const QString &mode);
    Q_INVOKABLE void openDraft(const QString &draftId, bool restoreView = false);
    Q_INVOKABLE bool addDraftRecipient(const QString &field, const QString &text);
    Q_INVOKABLE void stageDraftRecipient(const QString &field, const QString &text);
    Q_INVOKABLE void removeDraftRecipient(const QString &field, int index);
    Q_INVOKABLE void setDraftSubject(const QString &subject);
    Q_INVOKABLE void editFormattedDraft(const QString &body, const QString &html);
    Q_INVOKABLE void setDraftAttachment(const QString &id, bool selected);
    Q_INVOKABLE void setDraftSignature(const QString &signature);
    Q_INVOKABLE void discardDraft();
    Q_INVOKABLE void captureDraft();
    Q_INVOKABLE void inspectCapture(const QString &operationId);
    Q_INVOKABLE void loadMoreCaptures();
    Q_INVOKABLE void reloadDraft();
    Q_INVOKABLE void saveMyDraftVersion();
    Q_INVOKABLE bool validFilterSender(const QString &sender) const;
    Q_INVOKABLE void applyFilters(const QVariantMap &filters);
    Q_INVOKABLE void setCollapsedGroups(const QStringList &groups);
    Q_INVOKABLE void setPreferences(const QVariantMap &patch);
    Q_INVOKABLE void updateAccountPreferences(const QString &account, const QString &name, const QString &signature);
    Q_INVOKABLE void setCalendarSelected(const QString &account, const QString &calendar, bool selected);
    Q_INVOKABLE void setCalendarView(const QString &view);
    Q_INVOKABLE void moveCalendar(int delta);
    Q_INVOKABLE void setCalendarDate(const QString &date);
    Q_INVOKABLE void showCalendarToday();
    Q_INVOKABLE void moveTimeline(int delta);
    Q_INVOKABLE void showTimelineToday();
    Q_INVOKABLE void calendarWrite(const QVariantMap &values);
    Q_INVOKABLE void loadCalendarOperations();
    Q_INVOKABLE void loadMoreCalendarOperations();
    Q_INVOKABLE void retryCalendarOperation(const QString &account, const QString &requestId);
    Q_INVOKABLE void calendarOperationAction(const QString &account, const QString &requestId, const QString &action, qint64 expectedRevision = -1);
    Q_INVOKABLE QString calendarDateTime(const QString &date, const QString &time, const QString &zone) const;
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
    void googleAuthChanged();
    void googleSyncChanged();
    void mailDeliveryChanged();
    void googleLabelsChanged();
    void googleLabelBindingFinished(const QString &account, bool ok, const QVariantMap &result, const QString &error);
    void currentAccountChanged();
    void categoriesChanged();
    void conversationsChanged();
    void loadingChanged();
    void mutationChanged();
    void conversationChanged();
    void senderCardChanged();
    void draftChanged();
    void eventsChanged();
    void preferencesChanged();
    void capturesChanged();
    void calendarWriteFinished(bool ok, const QVariantMap &result, const QString &error);
    void calendarOperationsChanged();
    void accountChanged();
    void queryChanged();
    void errorChanged();
    void connectedChanged();
    void presentationChanged();
    void ready();
    void shutdownReady();
private:
    friend class GuiMailNavigationSelfTest;
    friend class GuiInteractionBenchmark;
    friend class GuiSettingsSelfTest;
    using Callback = std::function<void(const QJsonObject &, bool)>;
    struct Pending {
        Callback callback; qint64 started;
        QString method, account;
        int generation, listGeneration, calendarGeneration, senderGeneration;
        bool automaticRead = false;
        int queryBypasses = 0;
    };
    void request(const QString &method, const QString &account, const QJsonObject &params, Callback callback);
    void googleOperation(const QString &method, const QString &account, const QJsonObject &params, Callback callback);
    void cancelGoogleOperations(bool notify = true);
    void pollGoogleSync();
    void pollMailDelivery();
    void submitGoogleLabelBinding(const QString &account);
    void finishCalendarOperation(const QString &account, const QString &requestId, QJsonObject result, bool ok);
    void attemptConnection();
    void sendNext();
    void pruneSupersededQueries();
    void receive();
    void fail(const QString &message);
    void loadAccounts();
    void pollGoogleAuth();
    void setGoogleAuth(const QJsonObject &status);
    void refreshState(bool refreshList, bool refreshCalendar, bool automatic = false);
    void selectConversationInternal(const QString &id, bool review, QVariantMap clickedRow = {}, int clickedIndex = -1);
    void loadCategories();
    void loadConversations(bool append = false, bool preserveLoaded = false);
    void loadConversationPage(const QString &account, int generation, QJsonObject params, QVariantList collected, int desiredCount, int cursorRetries = 0);
    void loadConversation(bool loadDraft, bool reviewOnLoad = false);
    void loadCalendar();
    void loadCalendarOperationsPage(bool append);
    void loadPreferences();
    void loadCalendarSources();
    void loadCaptures(bool append = false);
    void savePreferences();
    void flag(const QString &name, bool value);
    void mutateConversation(const QString &method, QJsonObject params);
    void saveView(const QString &account);
    void cancelShutdown();
    void finishShutdown();
    QJsonObject mutation() const;
    QJsonObject draftSelector() const;
    QString draftContext() const;
    void loadDraftRows(bool append, bool preserveLoaded = true);
    void loadDraftPage(const QString &account, int generation, int offset, QVariantList collected, int desiredCount);
    void loadActiveDraft();
    void acceptDraft(const QJsonObject &result, bool restoreView = false);
    void beginDraftWithRecipient(const QString &mode, const QString &email, const QString &name);
    bool commitRecipientInputs();
    QString m_profile, m_engine, m_token, m_account, m_query, m_selected, m_error, m_category;
    QString m_folder = "inbox", m_weekStart = "2026-09-07", m_fixtureToday = "2026-09-11";
    QString m_draftStatus;
    QVariantMap m_googleAuth;
    QVariantMap m_googleSync;
    QVariantMap m_googleSyncLimits;
    QSet<GoogleOperation *> m_googleOperations;
    bool m_googleSyncPolling = false;
    QSet<QString> m_googleSyncChanging;
    QHash<QPair<QString, QString>, QVariantMap> m_mailDelivery;
    QList<QPair<QString, QString>> m_mailDeliveryOrder;
    bool m_mailDeliveryPolling = false;
    QVariantList m_googleLabels;
    QVariantMap m_googleLabelCatalog;
    QString m_googleLabelsAccount;
    int m_googleLabelsGeneration = 0;
    bool m_googleLabelsLoading = false;
    QSet<QString> m_googleLabelsRefreshing, m_googleLabelsBinding;
    QHash<QString, QJsonObject> m_googleLabelPendingBindings;
    QHash<QString, QString> m_googleLabelStatus;
    QString m_googleAuthAttempt;
    bool m_googleAuthBusy = false;
    bool m_googleAuthCancelRequested = false;
    QString m_activeDraft;
    QMap<QString, QString> m_recipientInputs;
    QJsonObject m_draftCreation;
    QString m_draftCreationAccount;
    QString m_draftCreationEmail;
    QVariantMap m_senderCard;
    int m_senderCardGeneration = 0;
    QVariantList m_accounts, m_conversations, m_events, m_todayEvents, m_categories;
    QVariantMap m_conversation, m_draft, m_views;
    QVariantMap m_preferences, m_preferencesPending, m_diagnostics, m_filters, m_querySummary;
    QStringList m_collapsedGroups;
    QVariantMap m_pinnedRow;
    int m_pinnedIndex = -1;
    QVariantList m_calendarSources, m_captures;
    QString m_preferencesStatus, m_captureStatus, m_nextCursor;
    int m_captureNextOffset = -1, m_captureGeneration = 0;
    QString m_calendarView = "week", m_calendarDate, m_timelineDate, m_calendarCoverage;
    QString m_timelineCoverage, m_calendarScope, m_timelineScope;
    bool m_calendarReady = false, m_timelineReady = false;
    QVariantList m_calendarOperations;
    QHash<QString, QJsonObject> m_calendarOperationCursors;
    QString m_calendarOperationStatus;
    bool m_calendarOperationsLoading = false;
    int m_calendarOperationsGeneration = 0;
    qint64 m_preferencesRevision = 0;
    qint64 m_calendarRevision = -1;
    int m_calendarGeneration = 0;
    int m_calendarWrites = 0, m_accountWrites = 0;
    bool m_preferencesSaving = false, m_capturing = false, m_discarding = false, m_fixtureClock = false;
    QJsonObject m_captureRequest;
    QString m_captureAccount, m_captureConversation;
    QTimer m_clockTimer;
    QHash<QString, qint64> m_viewRevisions;
    QHash<QString, QVariantMap> m_viewPending;
    QHash<QString, Pending> m_pending;
    QQueue<QPair<QString, QByteArray>> m_outgoing;
    QString m_inFlight;
    QLocalSocket m_socket;
    QTimer m_connectTimer, m_pollTimer, m_searchTimer, m_draftTimer, m_viewTimer;
    QByteArray m_buffer;
    QElapsedTimer m_startupClock;
    QElapsedTimer m_queryActivity;
    qint64 m_sequence = 0, m_revision = -1, m_enginePid = 0, m_nextLaunch = 0;
    int m_generation = 0, m_listGeneration = 0, m_nextOffset = -1;
    int m_desiredConversationCount = 50;
    int m_launchAttempts = 0, m_launchDelay = 1000;
    bool m_connected = false, m_loading = false;
    bool m_liveProfileRequested = false, m_syntheticProfile = true;
    bool m_listRefreshPending = false;
    bool m_automaticReadScope = false;
    bool m_mutatingConversation = false;
    bool m_draftDirty = false, m_savingDraft = false, m_shutdown = false;
    bool m_draftConflict = false, m_resolvingDraft = false;
    bool m_resumeSearch = false;
    bool m_shutdownEmitted = false;
    std::function<void()> m_afterDraftSave;
    std::function<void()> m_afterConversationMutation;
    QSet<QString> m_viewSaving, m_viewDirty;
};
