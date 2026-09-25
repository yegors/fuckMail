#pragma once
#include "ConversationListModel.h"

#include <QObject>
#include <QElapsedTimer>
#include <QJsonObject>
#include <QLocalSocket>
#include <QTimer>
#include <QDateTime>
#include <QTimeZone>
#include <QVariantList>
#include <QVariantMap>
#include <QSet>
#include <QQueue>
#include <QCache>
#include <QFuture>
#include <QThreadPool>
#include <QUrl>
#include <functional>

class GoogleOperation;
class HtmlContent;

// Phase 1: shared Core/Mail/Calendar boundary, additive protocol 1.
// Presentation and asynchronous IPC only. No direct database or provider access.
class MailBackend final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantList accounts READ accounts NOTIFY accountsChanged)
    Q_PROPERTY(bool startupReady READ startupReady NOTIFY startupReadyChanged)
    Q_PROPERTY(QVariantList allAccounts READ allAccounts NOTIFY accountsChanged)
    Q_PROPERTY(bool accountArchived READ accountArchived NOTIFY accountLifecycleChanged)
    Q_PROPERTY(QVariantMap accountLifecycle READ accountLifecycle NOTIFY accountLifecycleChanged)
    Q_PROPERTY(bool unsavedAccountBuffer READ unsavedAccountBuffer NOTIFY accountLifecycleChanged)
    Q_PROPERTY(QAbstractItemModel *accountModel READ accountModel CONSTANT)
    Q_PROPERTY(QVariantList conversations READ conversations NOTIFY conversationsChanged)
    Q_PROPERTY(QAbstractItemModel *conversationModel READ conversationModel CONSTANT)
    Q_PROPERTY(QAbstractItemModel *messageModel READ messageModel CONSTANT)
    Q_PROPERTY(QVariantList categories READ categories NOTIFY categoriesChanged)
    Q_PROPERTY(QVariantMap conversation READ conversation NOTIFY conversationChanged)
    Q_PROPERTY(QVariantMap senderCard READ senderCard NOTIFY senderCardChanged)
    Q_PROPERTY(QVariantMap draft READ draft NOTIFY draftChanged)
    Q_PROPERTY(QString activeDraftId READ activeDraftId NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap recipientInputs READ recipientInputs NOTIFY draftChanged)
    // Why the last typed recipient was not added, shown beside the fields.
    Q_PROPERTY(QString recipientError READ recipientError NOTIFY draftChanged)
    Q_PROPERTY(QVariantList events READ events NOTIFY eventsChanged)
    Q_PROPERTY(int calendarBandRows READ calendarBandRows NOTIFY eventsChanged)
    Q_PROPERTY(QVariantList todayEvents READ todayEvents NOTIFY timelineChanged)
    Q_PROPERTY(QVariantList reminders READ reminders NOTIFY remindersChanged)
    Q_PROPERTY(QString account READ account NOTIFY accountChanged)
    Q_PROPERTY(QVariantMap currentAccount READ currentAccount NOTIFY currentAccountChanged)
    Q_PROPERTY(QString query READ query WRITE setQuery NOTIFY queryChanged)
    Q_PROPERTY(QString folder READ folder WRITE setFolder NOTIFY queryChanged)
    Q_PROPERTY(QString category READ category WRITE setCategory NOTIFY queryChanged)
    Q_PROPERTY(QString selectedId READ selectedId NOTIFY conversationChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)
    Q_PROPERTY(QString profilePath READ profilePath CONSTANT)
    Q_PROPERTY(QVariantMap debugRuntime READ debugRuntime)
    Q_PROPERTY(QString draftStatus READ draftStatus NOTIFY draftChanged)
    Q_PROPERTY(bool connected READ connected NOTIFY connectedChanged)
    // The HTML reader: sanitised HTML, images, link cards, blocklist and proxy.
    Q_PROPERTY(QObject *htmlContent READ htmlContent CONSTANT)
    Q_PROPERTY(bool syntheticProfile READ syntheticProfile NOTIFY connectedChanged)
    Q_PROPERTY(QVariantMap googleAuth READ googleAuth NOTIFY googleAuthChanged)
    Q_PROPERTY(QVariantMap googleAuthConfiguration READ googleAuthConfiguration NOTIFY googleAuthChanged)
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
    Q_PROPERTY(QString fixtureToday READ fixtureToday NOTIFY todayChanged)
    Q_PROPERTY(QVariantMap presentation READ presentation NOTIFY presentationChanged)
    Q_PROPERTY(QVariantMap preferences READ preferences NOTIFY preferencesChanged)
    Q_PROPERTY(QString preferencesStatus READ preferencesStatus NOTIFY preferencesChanged)
    Q_PROPERTY(bool preferencesPending READ preferencesPending NOTIFY preferencesChanged)
    Q_PROPERTY(QVariantMap diagnostics READ diagnostics NOTIFY diagnosticsChanged)
    Q_PROPERTY(QVariantMap filters READ filters NOTIFY queryChanged)
    // Collapsed Inbox sections, saved per account: review, held or done:YYYY-MM.
    Q_PROPERTY(QStringList collapsedGroups READ collapsedGroups NOTIFY queryChanged)
    // How Core read the current search text, or why it could not.
    Q_PROPERTY(QVariantMap searchTerms READ searchTerms NOTIFY searchTermsChanged)
    Q_PROPERTY(QString searchError READ searchError NOTIFY searchTermsChanged)
    // Also search the account on the server; results not on this machine join the list.
    Q_PROPERTY(bool remoteSearch READ remoteSearch WRITE setRemoteSearch NOTIFY remoteSearchChanged)
    Q_PROPERTY(QVariantMap remoteSearchState READ remoteSearchState NOTIFY remoteSearchChanged)
    // Spam and Trash: Gmail's totals for the open one, and whether an action runs.
    Q_PROPERTY(QVariantMap binCounts READ binCounts NOTIFY binCountsChanged)
    Q_PROPERTY(bool binBusy READ binBusy NOTIFY binCountsChanged)
    Q_PROPERTY(QStringList recentSearches READ recentSearches NOTIFY recentSearchesChanged)
    Q_PROPERTY(QVariantList searchPeople READ searchPeople NOTIFY searchPeopleChanged)
    Q_PROPERTY(QString searchPeopleQuery READ searchPeopleQuery NOTIFY searchPeopleChanged)
    Q_PROPERTY(QVariantMap querySummary READ querySummary NOTIFY conversationsChanged)
    Q_PROPERTY(QVariantList captures READ captures NOTIFY capturesChanged)
    Q_PROPERTY(QString captureStatus READ captureStatus NOTIFY capturesChanged)
    Q_PROPERTY(bool capturesMore READ capturesMore NOTIFY capturesChanged)
    Q_PROPERTY(QVariantMap actionToast READ actionToast NOTIFY actionToastChanged)
    Q_PROPERTY(bool sendQueued READ sendQueued NOTIFY draftChanged)
    Q_PROPERTY(int sendingCount READ sendingCount NOTIFY conversationsChanged)
    Q_PROPERTY(QVariantMap draftTransfer READ draftTransfer NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap draftDiscard READ draftDiscard NOTIFY draftChanged)
    Q_PROPERTY(bool draftBusy READ draftBusy NOTIFY draftChanged)
    Q_PROPERTY(bool draftLocked READ draftLocked NOTIFY draftChanged)
    Q_PROPERTY(bool draftReadOnly READ draftReadOnly NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap googleDraftInventory READ googleDraftInventory NOTIFY googleDraftInventoryChanged)
    Q_PROPERTY(QVariantList draftSnapshotHeaders READ draftSnapshotHeaders NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap draftSources READ draftSources NOTIFY draftSourcesChanged)
    Q_PROPERTY(QVariantMap draftAuthoring READ draftAuthoring NOTIFY draftAuthoringChanged)
    Q_PROPERTY(QVariantMap draftDeletion READ draftDeletion NOTIFY draftDeletionChanged)
    Q_PROPERTY(bool draftConflict READ draftConflict NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap attachmentImport READ attachmentImport NOTIFY draftChanged)
    Q_PROPERTY(bool attachmentImportBusy READ attachmentImportBusy NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap googleDraft READ googleDraft NOTIFY draftChanged)
    Q_PROPERTY(QVariantMap googleSend READ googleSend NOTIFY draftChanged)
    Q_PROPERTY(QVariantList calendarSources READ calendarSources NOTIFY calendarSourcesChanged)
    Q_PROPERTY(QString calendarSearchQuery READ calendarSearchQuery WRITE setCalendarSearchQuery NOTIFY calendarSearchChanged)
    Q_PROPERTY(QVariantMap calendarSearch READ calendarSearch NOTIFY calendarSearchChanged)
    Q_PROPERTY(QVariantMap calendarMailActivity READ calendarMailActivity NOTIFY calendarMailActivityChanged)
    Q_PROPERTY(QString calendarView READ calendarView NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarDate READ calendarDate NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarFrom READ calendarFrom NOTIFY eventsChanged)
    Q_PROPERTY(int calendarDays READ calendarDays NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarCoverage READ calendarCoverage NOTIFY calendarStatusChanged)
    Q_PROPERTY(QString timelineCoverage READ timelineCoverage NOTIFY timelineChanged)
    Q_PROPERTY(bool calendarReady READ calendarReady NOTIFY calendarStatusChanged)
    Q_PROPERTY(bool timelineReady READ timelineReady NOTIFY timelineChanged)
    Q_PROPERTY(QVariantList timelinePanels READ timelinePanels NOTIFY timelineChanged)
    Q_PROPERTY(QVariantList calendarOperations READ calendarOperations NOTIFY calendarOperationsChanged)
    Q_PROPERTY(bool calendarOperationsLoading READ calendarOperationsLoading NOTIFY calendarOperationsChanged)
    Q_PROPERTY(bool calendarOperationsHasMore READ calendarOperationsHasMore NOTIFY calendarOperationsChanged)
    Q_PROPERTY(QString calendarOperationStatus READ calendarOperationStatus NOTIFY calendarOperationsChanged)
    Q_PROPERTY(QString timelineDate READ timelineDate NOTIFY timelineChanged)
    // The clock notifies only when its value changes: the date, or the minute.
    // Read live; the notify signals fire only when the displayed day or minute changes.
    Q_PROPERTY(QString todayDate READ todayDate NOTIFY todayChanged)
    Q_PROPERTY(QDateTime clockDay READ clockNow NOTIFY todayChanged)
    Q_PROPERTY(double nowHour READ nowHour NOTIFY clockChanged)
    Q_PROPERTY(QDateTime clockNow READ clockNow NOTIFY clockChanged)
public:
    explicit MailBackend(QString profile, QString engine, QObject *parent = nullptr, bool fixtureClock = false, bool liveProfile = false);
    ~MailBackend() override;
    QVariantList accounts() const { return m_accounts; }
    QVariantList allAccounts() const { return m_allAccounts; }
    bool startupReady() const { return m_initialAccountsLoaded && m_initialPreferencesLoaded; }
    bool accountArchived() const;
    bool accountRemoved(const QString &account) const;
    QVariantMap accountLifecycle() const;
    bool unsavedAccountBuffer() const { return m_unsavedAccountBuffer; }
    QAbstractItemModel *accountModel() { return &m_accountModel; }
    QVariantList conversations() const;
    QAbstractItemModel *conversationModel() { return &m_conversationModel; }
    QAbstractItemModel *messageModel() { return &m_messageModel; }
    QVariantList categories() const { return m_categories; }
    QVariantMap conversation() const;
    QVariantMap senderCard() const { return m_senderCard; }
    QVariantMap draft() const { return m_draft; }
    QString activeDraftId() const { return m_activeDraft; }
    QVariantMap recipientInputs() const { QVariantMap values; for (auto it = m_recipientInputs.cbegin(); it != m_recipientInputs.cend(); ++it) values[it.key()] = it.value(); return values; }
    QVariantList events() const { return visibleCalendarEvents(m_events, true); }
    int calendarBandRows() const;
    QVariantList todayEvents() const { return visibleCalendarEvents(m_todayEvents, true); }
    QVariantList timelinePanels() const;
    QVariantList reminders() const { return visibleCalendarEvents(m_reminders, false, false); }
    Q_INVOKABLE void dismissCalendarReminder(const QVariantMap &event);
    Q_INVOKABLE void joinCalendarEvent(const QVariantMap &event);
    Q_INVOKABLE void snoozeCalendarReminder(const QVariantMap &event);
    Q_INVOKABLE void playReminderSound(int level);
    Q_INVOKABLE void stopReminderSound();
    Q_INVOKABLE QString calendarReminderWhen(const QVariantMap &event) const;
    QVariantMap currentAccount() const;
    QString account() const { return m_account; }
    QString query() const { return m_query; }
    QString folder() const { return m_folder; }
    QString category() const { return m_category; }
    QString selectedId() const { return m_selected; }
    QString error() const { return m_error; }
    QVariantMap debugRuntime() const { return {{"pending", m_pending.size()}, {"queued", m_outgoing.size()}, {"in_flight", !m_inFlight.isEmpty()}, {"provider_operations", m_googleOperations.size()}, {"engine_pid", m_enginePid}, {"shutting_down", m_shutdown}}; }
    QString draftStatus() const { return m_draftStatus; }
    QString recipientError() const { return m_recipientError; }
    bool connected() const { return m_connected; }
    QObject *htmlContent() const;
    bool syntheticProfile() const { return m_syntheticProfile; }
    QVariantMap googleAuth() const { return m_googleAuth; }
    QVariantMap googleAuthConfiguration() const { return m_googleAuthConfiguration; }
    bool googleAuthBusy() const { return m_googleAuthBusy; }
    QVariantMap googleSync() const { return m_googleSync; }
    QVariantMap googleSyncLimits() const { return m_googleSyncLimits; }
    QVariantMap mailDelivery() const { return m_mailDelivery.value({m_account, m_selected}); }
    QVariantList googleLabels() const { return m_googleLabels; }
    QVariantMap googleLabelState() const;
    bool loading() const { return m_loading; }
    Q_INVOKABLE void prefetchMore();
    Q_INVOKABLE void prefetchConversation(const QString &id);
    Q_INVOKABLE void setBrowsingOlder(bool value) { m_browsingOlder = value; }
    QString listError() const { return m_listError; }
    Q_PROPERTY(QString listError READ listError NOTIFY loadingChanged)
    bool mutationInFlight() const { return m_mutatingConversation; }
    bool hasMore() const { return m_nextOffset >= 0 || (m_folder == "sent" && m_outgoingNext.value(m_account, -1) >= 0); }
    QString weekStart() const { return m_weekStart; }
    QString fixtureToday() const { return m_fixtureToday; }
    QVariantMap presentation() const { return m_views.value(m_account).toMap(); }
    QVariantMap preferences() const { return m_preferences; }
    QString preferencesStatus() const { return m_preferencesStatus; }
    bool preferencesPending() const { return !m_preferencesPending.isEmpty(); }
    QVariantMap diagnostics() const { return m_diagnostics; }
    QVariantMap filters() const { return m_filters; }
    QStringList collapsedGroups() const { return m_collapsedGroups; }
    QVariantMap searchTerms() const { return m_searchTerms; }
    QString searchError() const { return m_searchError; }
    QStringList recentSearches() const { return presentation().value("recentSearches").toStringList(); }
    QVariantList searchPeople() const { return m_searchPeople; }
    QString searchPeopleQuery() const { return m_searchPeopleQuery; }
    QVariantMap querySummary() const { return m_querySummary; }
    QVariantList captures() const { return m_captures; }
    QString captureStatus() const { return m_captureStatus; }
    bool capturesMore() const { return m_captureNextOffset >= 0; }
    bool draftBusy() const { return m_savingDraft || m_capturing || m_discarding || m_undoingDiscards || draftDiscardActive() || m_resolvingDraft || !m_draftTransferActive.isEmpty(); }
    bool draftReadOnly() const { return m_draft.value("origin").toString() == "google_snapshot" || (m_draft.value("read_only").toBool() && !m_draft.value("account_archived").toBool()); }
    bool draftLocked() const { return accountArchived() || m_unsavedAccountBuffer || draftReadOnly() || draftTransferPending() || draftDiscardPending() || m_undoingDiscards || sendQueued() || m_capturing || m_discarding || m_resolvingDraft; }
    bool draftConflict() const { return m_draftConflict; }
    QVariantList calendarSources() const { return m_calendarSources; }
    QString calendarSearchQuery() const { return m_calendarSearchQuery; }
    QVariantMap calendarSearch() const;
    void setCalendarSearchQuery(const QString &query);
    Q_INVOKABLE void setCalendarSearchContext(bool active, bool combined);
    Q_INVOKABLE void setCalendarMailActivityContext(bool active, bool combined);
    QVariantMap calendarMailActivity() const { return m_calendarMailActivity; }
    Q_INVOKABLE void openCalendarSearchResult(const QVariantMap &event);
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
    Q_INVOKABLE void revealProfile();
    QString profilePath() const;
    Q_INVOKABLE void selectAccount(const QString &id);
    Q_INVOKABLE void selectConversation(const QString &id);
    Q_INVOKABLE void openConversation(const QString &id);
    Q_INVOKABLE void requestSenderCard(const QString &email);
    Q_INVOKABLE void clearSenderCard();
    Q_INVOKABLE void writeToSender(const QString &email, const QString &name);
    Q_INVOKABLE void loadMore();
    Q_INVOKABLE QString assistanceRequest(const QString &method, const QVariantMap &params, const QString &account = QString());
    // Live Gmail Spam for the current account: google_spam_list or google_spam_message.
    // Unsubscribe from the list of a received message the reader shows.
    Q_INVOKABLE QString unsubscribe(const QVariantMap &message);
    // The open Spam conversation to the Inbox; the open Spam or Trash
    // conversation, or the whole folder, deleted for good.
    Q_INVOKABLE void notSpam();
    Q_INVOKABLE void deleteForever();
    Q_INVOKABLE void emptyBin();
    Q_INVOKABLE QString labelRequest(const QString &method, const QVariantMap &params, const QString &account);
    Q_INVOKABLE QString requestAttachmentPreview(const QString &account, const QString &id);
    Q_INVOKABLE QString saveAttachment(const QString &account, const QString &id, const QUrl &destination);
    Q_INVOKABLE QString downloadAttachment(const QString &account, const QString &id, const QString &name);
    Q_INVOKABLE QUrl attachmentSaveLocation(const QString &name) const;
    // withRaw adds the raw text for the source dialog; it is never cached.
    Q_INVOKABLE QString requestMessageSource(const QString &id, const QString &version, bool withRaw = false);
    Q_INVOKABLE QString receivedHtml(const QString &html) const;
    Q_INVOKABLE QVariantList receivedBody(const QString &html, const QString &plain) const;
    Q_INVOKABLE QString fixedFontFamily() const;
    Q_INVOKABLE void copyText(const QString &text);
    Q_INVOKABLE void refresh();
    Q_INVOKABLE void prepareGoogleSignIn();
    Q_INVOKABLE void connectGoogle(const QString &email, const QString &account = {}, bool reauthorize = false, const QVariantMap &access = {});
    Q_INVOKABLE void cancelGoogleAuth();
    Q_INVOKABLE void disconnectGoogle(const QString &account);
    Q_INVOKABLE void previewAccountRemoval(const QString &account);
    Q_INVOKABLE void previewArchiveDeletion(const QString &account);
    Q_INVOKABLE void confirmArchiveDeletion(const QVariantMap &context);
    Q_INVOKABLE void confirmAccountRemoval(const QVariantMap &context);
    Q_INVOKABLE void retryAccountCleanup(const QString &account);
    Q_INVOKABLE void checkAccountRemoval(const QString &account, const QString &requestId);
    Q_INVOKABLE void openAccountArchive(const QString &account);
    Q_INVOKABLE void restoreGoogleAccount(const QString &email, const QString &account, qint64 expectedRevision, bool reauthorize = false, const QVariantMap &access = {});
    Q_INVOKABLE bool accountNavigationAllowed();
    Q_INVOKABLE void resolveUnsavedAccountBuffer(bool discard);
    Q_INVOKABLE bool accountCapability(const QString &account, const QString &name) const;
    Q_INVOKABLE void refreshGoogleSync(const QString &account);
    Q_INVOKABLE QVariantMap googleSyncState(const QString &account) const;
    Q_INVOKABLE void startGoogleSync(const QString &account, const QVariantMap &limits);
    Q_INVOKABLE void pauseGoogleSync(const QString &account);
    Q_INVOKABLE void cancelGoogleOperation(const QString &account, const QString &requestId);
    Q_INVOKABLE void loadGoogleLabels(const QString &account, bool append = false);
    Q_INVOKABLE void refreshGoogleLabels(const QString &account);
    Q_INVOKABLE void bindGoogleLabel(const QString &account, const QString &labelId, const QString &categoryId, const QString &createName, const QVariant &expectedProviderLabelId, qint64 expectedCatalogRevision);
    Q_INVOKABLE void retryGoogleLabelBinding(const QString &account);
    Q_INVOKABLE QVariantList snapshotMailRows(const QStringList &ids) const;
    Q_INVOKABLE void applyMailSelection(const QVariantList &rows, const QString &action, const QString &categoryId = {});
    Q_PROPERTY(int mailScopeCount READ mailScopeCount NOTIFY conversationsChanged)
    int mailScopeCount() const { return m_mailScopeCount; }
    Q_PROPERTY(QString mailSelectionStatus READ mailSelectionStatus NOTIFY mutationChanged)
    QString mailSelectionStatus() const { return m_mailSelectionStatus; }
    Q_INVOKABLE void setRead(bool read);
    Q_INVOKABLE void setStarred(bool starred);
    Q_INVOKABLE void doneConversation();
    Q_INVOKABLE void dismissMailUndo();
    Q_INVOKABLE void setCategoryMembership(const QString &categoryId, bool present);
    Q_INVOKABLE void fileConversation(const QString &action);
    Q_INVOKABLE void editDraft(const QString &body);
    Q_INVOKABLE void beginDraft(const QString &mode);
    QVariantMap actionToast() const;
    bool sendQueued() const;
    Q_INVOKABLE void activateActionToast();
    Q_INVOKABLE void outgoingAction(const QString &id, bool undo = false);
    int sendingCount() const;
    QVariantMap draftTransfer() const;
    bool draftTransferPending() const;
    Q_INVOKABLE void changeDraftAccount(const QString &account);
    Q_INVOKABLE void retryDraftTransfer();
    Q_INVOKABLE void retryDraftTransferCleanup();
    Q_INVOKABLE void openDraftTransferDestination();
    Q_INVOKABLE void openDraft(const QString &draftId, bool restoreView = false);
    Q_INVOKABLE bool addDraftRecipient(const QString &field, const QString &text);
    Q_INVOKABLE bool addDraftAddress(const QString &field, const QString &email, const QString &name);
    Q_INVOKABLE QString recipientSuggestions(const QString &query, const QStringList &exclude);
    Q_INVOKABLE void stageDraftRecipient(const QString &field, const QString &text);
    Q_INVOKABLE void removeDraftRecipient(const QString &field, int index);
    Q_INVOKABLE void setDraftSubject(const QString &subject);
    Q_INVOKABLE void editFormattedDraft(const QString &body, const QString &html);
    Q_INVOKABLE void setDraftAttachment(const QString &id, bool selected);
    Q_INVOKABLE void importDraftAttachment(const QUrl &file, const QString &account, const QString &draftId);
    Q_INVOKABLE void importDraftAttachments(const QList<QUrl> &files, const QString &account, const QString &draftId);
    Q_INVOKABLE void retryDraftAttachment();
    Q_INVOKABLE void skipQueuedDraftAttachments();
    QVariantMap attachmentImport() const;
    bool attachmentImportBusy() const { return !m_attachmentImportActive.isEmpty(); }
    QVariantMap googleDraft() const;
    Q_INVOKABLE void reconcileGoogleDraft();
    QVariantMap googleDraftInventory() const;
    QVariantList draftSnapshotHeaders() const;
    QVariantMap draftSources() const;
    Q_INVOKABLE void loadGoogleDraftInventory();
    Q_INVOKABLE void refreshGoogleDraftInventory();
    Q_INVOKABLE void loadDraftSources(bool append = false);
    Q_INVOKABLE void exportDraftSource(qint64 sourceRevision, const QString &attachmentId = {});
    Q_INVOKABLE void showDraftSourceFolder();
    QVariantMap draftAuthoring() const;
    QVariantMap draftDeletion() const;
    Q_INVOKABLE void loadDraftAuthoring();
    Q_INVOKABLE QVariantMap draftSourceActionContext(const QString &action);
    Q_INVOKABLE void confirmDraftSourceAction(const QVariantMap &context);
    Q_INVOKABLE void retryDraftSourceAction();
    Q_INVOKABLE void openDraftSourceActionResult();
    Q_INVOKABLE void loadDraftDeletion(const QString &requestId = {});
    Q_INVOKABLE void checkDraftDeletion();
    Q_INVOKABLE QVariantMap draftDeletionAbandonContext();
    Q_INVOKABLE void abandonDraftDeletion(const QVariantMap &context);
    Q_INVOKABLE QVariantMap googleDraftReplacementContext();
    Q_INVOKABLE void replaceGoogleDraft(const QVariantMap &context);
    Q_INVOKABLE void cancelGoogleDraftOperation();
    QVariantMap googleSend() const;
    Q_INVOKABLE void sendGoogleDraft();
    Q_INVOKABLE void checkGoogleSend();
    Q_INVOKABLE void showCurrentGoogleSend();
    Q_INVOKABLE void resumeGoogleSend();
    Q_INVOKABLE void abandonGoogleSend();
    Q_INVOKABLE void cancelGoogleSend();
    Q_INVOKABLE void setDraftSignature(const QString &signature);
    Q_INVOKABLE void discardDraft(const QVariantMap &context = {});
    Q_INVOKABLE QVariantMap draftDiscardContext() const;
    Q_INVOKABLE void checkDraftDiscard();
    Q_INVOKABLE void retryDraftDiscard();
    QVariantMap draftDiscard() const;
    Q_INVOKABLE void captureDraft();
    Q_INVOKABLE void inspectCapture(const QString &operationId);
    Q_INVOKABLE void loadMoreCaptures();
    Q_INVOKABLE void reloadDraft();
    Q_INVOKABLE void saveMyDraftVersion();
    Q_INVOKABLE bool validFilterSender(const QString &sender) const;
    Q_INVOKABLE void applyFilters(const QVariantMap &filters);
    Q_INVOKABLE void setCollapsedGroups(const QStringList &groups);
    // Enter keeps the search and adds it to this account's recent searches.
    Q_INVOKABLE void commitSearch(const QString &text);
    Q_INVOKABLE void removeRecentSearch(const QString &text);
    // People after from:; senders, or recipients in Sent and Drafts.
    Q_INVOKABLE void requestSearchPeople(const QString &part);
    Q_INVOKABLE void setPreferences(const QVariantMap &patch);
    Q_INVOKABLE void updateAccountPreferences(const QString &account, const QString &name, const QString &signature, const QString &retention = {}, const QVariantMap &sync = {}, bool analyzeHistory = false, const QString &color = {});
    Q_INVOKABLE void setCalendarSelected(const QString &account, const QString &calendar, bool selected);
    Q_INVOKABLE void setCalendarView(const QString &view);
    Q_INVOKABLE void moveCalendar(int delta);
    Q_INVOKABLE void setCalendarDate(const QString &date);
    // One update for a month-day click instead of a date change then a view change.
    Q_INVOKABLE void openCalendarDay(const QString &date);
    Q_INVOKABLE void openCalendarWeek(const QString &date);
    Q_INVOKABLE void showCalendarToday();
    Q_INVOKABLE void moveTimeline(int delta);
    Q_INVOKABLE void showTimelineToday();
    Q_INVOKABLE void calendarWrite(const QVariantMap &values);
    Q_INVOKABLE QString queueCalendarDeletion(const QVariantMap &values, const QVariantMap &event);
    Q_INVOKABLE QVariantMap calendarOccurrenceContext(const QString &account, const QString &calendar) const;
    Q_INVOKABLE void readCalendarOccurrence(const QVariantMap &selector, const QString &correlation);
    Q_INVOKABLE void loadCalendarOperations();
    Q_INVOKABLE void loadMoreCalendarOperations();
    Q_INVOKABLE void retryCalendarOperation(const QString &account, const QString &requestId);
    Q_INVOKABLE void calendarOperationAction(const QString &account, const QString &requestId, const QString &action, qint64 expectedRevision = -1);
    Q_INVOKABLE QString calendarDateTime(const QString &date, const QString &time, const QString &zone) const;
    Q_INVOKABLE QVariantList calendarPreview(const QVariantMap &event) const;
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
    void calendarMailActivityChanged();
    // Metadata only. Never attach request params, response bodies or account IDs.
    void debugEvent(const QString &level, const QString &source, const QString &message, const QVariantMap &fields);
    void recipientSuggestionsReply(const QString &ticket, const QVariantMap &result, bool ok);
    void assistanceReply(const QString &ticket, const QString &method, const QVariantMap &result, bool ok);
    void remoteSearchChanged();
    void unsubscribeReady(const QString &ticket, const QVariantMap &result, bool ok);
    void binCountsChanged();
    void binActionFinished(const QString &action, bool ok, const QVariantMap &result);
    void attachmentPreviewReady(const QString &ticket, const QVariantMap &result, bool ok);
    void attachmentSaved(const QString &ticket, const QString &error);
    void mailSelectionRestored(const QVariantList &rows);
    void messageSourceReady(const QString &ticket, const QVariantMap &result, bool ok);
    void calendarSearchChanged();
    void calendarSearchResultReady(const QVariantMap &event);
    void remindersChanged();
    void desktopNotification(const QString &title, const QString &body);
    void startupReadyChanged();
    void accountsChanged();
    void accountLifecycleChanged();
    void accountRemovalReady(const QVariantMap &context);
    void archiveDeletionReady(const QVariantMap &context);
    void accountBufferDispositionRequired();
    void googleAuthChanged();
    void googleSyncChanged();
    void mailDeliveryChanged();
    void googleLabelsChanged();
    void labelReply(const QString &ticket, const QString &method, const QVariantMap &result, bool ok);
    void googleLabelBindingFinished(const QString &account, bool ok, const QVariantMap &result, const QString &error);
    void currentAccountChanged();
    void categoriesChanged();
    void conversationsChanged();
    void loadingChanged();
    void mutationChanged();
    void conversationChanged();
    void senderCardChanged();
    void actionToastChanged();
    void draftChanged();
    void googleDraftInventoryChanged();
    void draftSourcesChanged();
    void draftAuthoringChanged();
    void draftDeletionChanged();
    // Calendar window: events plus the view, date and range they belong to.
    void eventsChanged();
    void calendarStatusChanged();
    void calendarSourcesChanged();
    void timelineChanged();
    void todayChanged();
    void clockChanged();
    void diagnosticsChanged();
    void preferencesChanged();
    void accountPreferencesFinished(const QString &account, bool ok);
    void capturesChanged();
    void calendarWriteFinished(bool ok, const QVariantMap &result, const QString &error);
    void calendarActivityRequested();
    void calendarOccurrenceReadFinished(const QString &correlation, bool ok, const QVariantMap &result, const QString &error);
    void calendarOperationsChanged();
    void accountChanged();
    void queryChanged();
    void searchTermsChanged();
    void recentSearchesChanged();
    void searchPeopleChanged();
    void errorChanged();
    void connectedChanged();
    void presentationChanged();
    void ready();
    void shutdownReady();
private:
    friend class CalendarSearchTest;
    friend class CalendarUndoTest;
    friend class MailNavigationCacheTest;
    friend class MailSelectionTest;
    friend class NotificationBackendTest;
    friend class GuiMailNavigationSelfTest;
    friend class GuiInteractionBenchmark;
    friend class GuiCalendarBenchmark;
    friend class GuiSettingsSelfTest;
    friend class GuiCompositionSelfTest;
    friend class GoogleDraftControlsTest;
    friend class GoogleDraftSnapshotControlsTest;
    friend class GoogleDraftAuthoringControlsTest;
    friend class GuiGoogleDraftSnapshotSelfTest;
    friend class GoogleSendControlsTest;
    friend class ComposerInputTest;
    friend class DraftTransferControlsTest;
    friend class SendUndoTest;
    friend class DraftDiscardTest;
    friend class CalendarOccurrenceBackendTest;
    friend class CalendarMailActivityTest;
    friend class AccountLifecycleControlsTest;
    friend class GuiAccountLifecycleSelfTest;
    friend class HtmlContent;
    using Callback = std::function<void(const QJsonObject &, bool)>;
    struct Pending {
        Callback callback; qint64 started;
        QString method, account;
        int generation, listGeneration, calendarGeneration, senderGeneration;
        bool automaticRead = false;
        int queryBypasses = 0;
        // First page of a list search: the search dropdown explains a refusal.
        bool searchText = false;
        // A queued Send's draft check targets its own account and draft, not the
        // visible mailbox, so navigation never cancels it before the engine.
        bool retainOnNavigation = false;
    };
    void request(const QString &method, const QString &account, const QJsonObject &params, Callback callback, bool retainOnNavigation = false);
    void requestReader(const QString &account, const QJsonObject &params, Callback callback,
                       const QString &method = "conversation", bool prefetch = false);
    GoogleOperation *m_readerOperation = nullptr;
    GoogleOperation *m_readerPrefetchOperation = nullptr;
    QString m_readerPrefetchTarget;
    QTimer m_readerPrefetchTimer;
    QThreadPool m_readerPool;
    QFuture<QVariantMap> m_readerFuture, m_readerPrefetchFuture;
    quint64 m_readerLoadSerial = 0;
    int m_readerLoadingGeneration = -1;
    bool m_readerReviewPending = false;
    void prepareReader(const QString &account, const QString &id, QVariantMap data, int generation,
                       qint64 lifecycle, bool prefetch, std::function<void(QVariantMap)> ready);
    QVariantMap retainReaderPresentation(QVariantMap data) const;
    void googleOperation(const QString &method, const QString &account, const QJsonObject &params, Callback callback);
    void attachmentFile(const QString &account, const QString &id, Callback callback);
    void cancelGoogleOperations(bool notify = true);
    void cancelGoogleProviderOperations(const QString &account);
    void pollGoogleSync();
    void resumeStartupGoogleSync();
    QSet<QString> m_startupGoogleAccounts;
    void pollMailDelivery();
    void submitGoogleLabelBinding(const QString &account);
    void finishCalendarOperation(const QString &account, const QString &requestId, QJsonObject result, bool ok);
    struct CalendarDeletion {
        QVariantMap request, source, authority;
        qint64 deadline = 0;
        bool dispatched = false;
    };
    QHash<QString, CalendarDeletion> m_calendarDeletions;
    int m_calendarUndoMs = 6000;
    QVariantMap currentCalendarSource(const QVariantMap &event) const;
    QVariantList visibleCalendarEvents(const QVariantList &rows, bool reflow = false, bool filterByAccount = true) const;
    bool calendarEventHidden(const QVariantMap &event) const;
    void advanceCalendarDeletions();
    void undoCalendarDeletion(const QString &key);
    void cancelQueuedCalendarDeletions();
    void calendarDeletionVisibilityChanged();
    void finishQueuedCalendarDeletion(const QString &account, const QString &id, bool confirmed, const QString &message);
    void attemptConnection();
    void sendNext();
    void pruneSupersededQueries();
    void receive();
    void fail(const QString &message);
    void loadAccounts();
    void pollGoogleAuth();
    void setGoogleAuth(const QJsonObject &status);
    void beginGoogleAuth(const QString &method, const QString &email, const QString &account, bool reauthorize, qint64 expectedRevision = -1, const QVariantMap &access = {});
    QVariantMap lifecycleAccount(const QString &account) const;
    bool archivedRequestAllowed(const QString &method) const;
    bool blockArchivedRequest(const QString &method, const QString &account) const;
    void reconcileAccountLifecycle();
    void freezeRemovedAccountBuffer();
    void clearAccountSelection();
    void submitAccountRemoval(const QString &account, const QJsonObject &params);
    void resumeAccountRemovalAfterView(const QString &account, int generation, bool restored);
    void refreshState(bool refreshList, bool refreshCalendar, bool automatic = false);
    void selectConversationInternal(const QString &id, bool review, QVariantMap clickedRow = {}, int clickedIndex = -1);
    void loadCategories();
    bool publishAccounts(QVariantList accounts);
    void loadConversations(bool append = false, bool preserveLoaded = false);
    bool publishConversations(QVariantList rows);
    QVariantMap mailViewScope() const;
    void cacheMailView();
    bool restoreMailView();
    void cacheConversation(const QString &account, const QString &id, const QVariantMap &data, qint64 lifecycle);
    bool restoreConversation(const QString &account, const QString &id);
    void loadConversationPage(const QString &account, int generation, QJsonObject params, QVariantList collected, int desiredCount, int cursorRetries = 0);
    void loadConversation(bool loadDraft, bool reviewOnLoad = false);
    // Force a fresh read of the visible calendar and timeline; cached windows are dropped.
    void loadCalendar();
    // Navigation: show a cached window at once, else request it.
    void showCalendarRange();
    void loadTimeline(bool force);
    struct CalendarWindow { QString from; int days = 0; QJsonObject params; QString key; };
    CalendarWindow calendarWindow(const QString &from, int days) const;
    CalendarWindow calendarBaseWindow(const QString &date) const;
    QString calendarRangeStart(const QString &view, const QString &date) const;
    void publishCalendarWindow(const QJsonObject &result, const QString &from, int days);
    void presentCalendarWeek(const QJsonArray &events, const QTimeZone &zone);
    void requestCalendarWindow(const CalendarWindow &window);
    void storeCalendarWindow(const QString &key, const QJsonObject &result);
    bool publishCachedCalendar();
    void prefetchCalendarWindows();
    void setCalendarStatus(bool ready, const QString &coverage);
    void rememberCalendarNavigation(const QVariantMap &values);
    void tickClock();
    QTimeZone clockZone() const;
    QString calendarCoverageText(const QJsonObject &coverage) const;
    void initializeCalendarSearch();
    void refreshCalendarMailActivity();
    QTimer m_calendarMailActivityTimer;
    QVariantMap m_calendarMailActivity;
    QString m_calendarMailActivityScope, m_calendarMailActivityStamp;
    bool m_calendarMailActivityActive = false, m_calendarMailActivityCombined = true, m_calendarMailActivityBusy = false;
    int m_calendarMailActivityGeneration = 0;
    void refreshCalendarSearchContext();
    void runCalendarSearch();
    QJsonObject calendarSearchParams() const;
    QString calendarSearchContextStamp() const;
    void loadCalendarOperationsPage(bool append);
    void loadPreferences();
    void configureNotifications();
    QString notificationContext() const;
    void pollNotifications();
    void pollMailNotifications();
    void acknowledgeCalendarReminder(const QVariantMap &event, bool join, bool snooze = false);
    void loadCalendarSources();
    void loadCaptures(bool append = false);
    void savePreferences();
    void flag(const QString &name, bool value);
    void mutateConversation(const QString &method, QJsonObject params, bool undoable = true);
    QVariantMap mailUndoToken(const QString &account, const QJsonObject &result) const;
    void offerMailUndo(const QString &label, const QVariantList &tokens, const QVariantList &selection = {});
    void undoMailActions(const QVariantMap &context);
    void runMailUndo();
    QString mailActionLabel(const QString &method, const QJsonObject &params, int count = 1) const;
    QVariantList m_bulkMailUndo, m_bulkMailSelection, m_pendingMailUndo, m_restoredMailSelection;
    QString m_bulkMailAction;
    quint64 m_mailUndoEpoch = 0, m_bulkMailUndoEpoch = 0;
    QVariantList m_mailUndoSelection;
    void saveView(const QString &account);
    void cancelShutdown();
    void finishShutdown();
    QJsonObject mutation() const;
    QJsonObject draftSelector() const;
    QString draftContext() const;
    void loadDraftRows(bool append, bool preserveLoaded = true);
    QJsonValue effectiveCalendarSelection() const;
    void loadDraftPage(const QString &account, int generation, int offset, QVariantList collected, int desiredCount);
    void loadActiveDraft();
    void acceptDraft(const QJsonObject &result, bool restoreView = false);
    bool olderDraftProjection(const QJsonObject &result) const;
    QVariantMap mergedDraftProjection(const QJsonObject &result) const;
    bool mergeGoogleDraftStatus(const QJsonObject &status, const QString &account, const QString &draftId);
    void startGoogleDraftOperation(const QString &method, const QJsonObject &params);
    bool googleDraftRecoveryReady() const;
    bool googleSendBlocksDraftSync() const;
    bool mergeGoogleSendStatus(const QJsonObject &status, const QString &account, const QString &draftId);
    void mergeGoogleSendProjection(QVariantMap &next, const QJsonObject &result) const;
    QString googleSendKey() const;
    QJsonObject googleSendAuthorSnapshot() const;
    void cancelPendingGoogleSend(const QString &reason = {});
    void finishGoogleSendSave(bool ok);
    void startGoogleSendOperation(const QString &method, const QJsonObject &params);
    void startGoogleSendForDraft(const QString &method, const QJsonObject &params, const QString &account, const QString &draftId);
    void refreshGoogleSendMetadata(const QString &account, const QString &draftId);
    struct SendUi {
        QJsonObject selected, current, request;
        QString method, feedback;
        bool active = false, uncertain = false, notFound = false;
    };
    QHash<QString, SendUi> m_googleSendUi;
    QJsonObject m_googleSendPending;
    struct QueuedSend {
        QJsonObject params;
        QVariantMap preview;
        QString account, recipientNames, from;
        qint64 lifecycle = 0, deadline = 0, nextCheck = 0;
        bool checking = false;
    };
    QHash<QString, QueuedSend> m_queuedSends;
    QHash<QString, QString> m_sendNoticeRecipients;
    QHash<QString, QVariantList> m_outgoingPreviews;
    QVariantList m_outgoingThread;
    QString m_outgoingThreadKey;
    QHash<QString, QVariantMap> m_outgoingBodies, m_transientOutgoing;
    QHash<QString, int> m_outgoingNext, m_outgoingGeneration;
    QSet<QString> m_outgoingReads;
    QVariantMap queuedOutgoing(const QJsonObject &params) const;
    QVariantList outgoingMessages() const;
    QVariantMap outgoingMessage(const QVariantMap &value) const;
    void refreshOutgoingPresentation();
    void loadOutgoing(bool more = false);
    void loadOutgoingThread();
    void loadOutgoingPage(const QString &account, const QVariant &lifecycle, int generation, int offset, int desired, QVariantList rows);
    void loadOutgoingMessage(const QVariantMap &value);
    QElapsedTimer m_actionClock;
    QTimer m_actionTimer;
    int m_sendUndoMs = 0; // No artificial send delay; negative bypasses preflight in isolated control tests.
    QVariantMap m_actionToast;
    QList<QVariantMap> m_deferredActionToasts;
    qint64 m_actionToastDeadline = 0;
    void initializeSendUndo();
    void queueGoogleSend(const QJsonObject &params);
    void advanceSendUndo();
    void dispatchQueuedSend(const QString &key);
    void cancelQueuedSend(const QString &key, bool restore);
    void cancelQueuedSends(const QString &reason);
    void showActionToast(const QString &text, const QString &action = {}, const QVariantMap &context = {}, int duration = 2800, bool preserveUndo = false);
    void finishQueuedSendNotice(const QString &key, const QJsonObject &result, bool valid);
    struct DiscardUi {
        QString method, feedback;
        QJsonObject params;
        qint64 lifecycle = 0, issuedAt = 0;
        bool active = false, notFound = false, reopen = false;
    };
    QHash<QString, DiscardUi> m_draftDiscardUi;
    QSet<QString> m_bulkDiscardTargets;
    QVariantList m_bulkDiscardUndo;
    qint64 m_bulkIssuedAt = 0;
    QList<QJsonObject> m_undoDiscardCommands;
    QVariantMap m_undoDiscardReopen;
    bool m_undoingDiscards = false;
    int m_undoDiscardCompleted = 0, m_undoDiscardTotal = 0;
    bool draftDiscardPending() const;
    bool draftDiscardActive() const;
    QJsonObject discardEditorBuffer() const;
    QVariantMap discardUndoToken(const QString &account, const QJsonObject &result, bool reopen) const;
    void startDraftDiscardRequest(const QString &method, const QString &account, const QJsonObject &params, qint64 issuedAt, bool reopen, Callback done);
    void offerDiscardUndo(const QVariantList &tokens, qint64 issuedAt);
    void finishBulkDiscardUndo();
    void undoDraftDiscards(const QVariantList &tokens);
    void runDiscardUndo();

    QJsonObject m_googleDraftReplacement;
    QString m_googleDraftOperationAccount, m_googleDraftOperationId, m_googleDraftOperationMethod;
    QString m_googleDraftFeedbackAccount, m_googleDraftFeedbackId, m_googleDraftFeedback;
    QSet<QString> m_googleDraftRequiresCheck;
    QString draftSourcesKey() const;
    void resetDraftSourcesContext();
    void resetDraftAuthoringContext();
    void startDraftSourceAction(const QString &key);
    bool mergeDraftDeletion(const QJsonObject &status, const QString &account, const QString &draftId);
    void mergeDraftDeletionProjection(QVariantMap &next, const QJsonObject &result) const;
    struct SourceActionUi {
        QJsonObject request;
        QJsonObject source;
        QString action, feedback, resultDraftId;
        bool active = false, uncertain = false;
        bool freshDeleteFinished = false;
    };
    QString m_draftAuthoringKey;
    QJsonObject m_draftAuthoring, m_draftSourceConfirmation, m_draftDeleteConfirmation;
    QString m_draftAuthoringFeedback;
    quint64 m_draftAuthoringGeneration = 0;
    bool m_draftAuthoringLoading = false;
    QHash<QString, SourceActionUi> m_draftSourceActions;
    QJsonObject m_draftDeletionSelected;
    QString m_draftDeletionAccount, m_draftDeletionDraft, m_draftDeletionFeedback;
    quint64 m_draftDeletionGeneration = 0;
    bool m_draftDeletionLoading = false, m_draftDeletionActive = false;
    QVariantMap m_draftInventory;
    QString m_draftInventoryAccount, m_draftInventoryFeedback;
    int m_draftInventoryGeneration = 0;
    bool m_draftInventoryLoading = false, m_draftInventoryRefreshing = false;
    QString m_draftSourcesKey, m_draftSourceFeedback;
    QVariantMap m_draftSources, m_draftSourceExport;
    int m_draftSourcesGeneration = 0;
    quint64 m_draftSourceExportGeneration = 0;
    bool m_draftSourcesLoading = false, m_draftSourceExporting = false;
    void beginDraftWithRecipient(const QString &mode, const QString &email, const QString &name);
    bool commitRecipientInputs();
    QString attachmentImportKey() const;
    bool attachmentImportPending() const;
    void startDraftAttachmentImport(const QString &key);
    void advanceDraftAttachments();
    QHash<QString, QList<QUrl>> m_attachmentQueues;
    QHash<QString, qint64> m_attachmentQueueLifecycle;
    QSet<QString> m_attachmentQueuePaused;
    QHash<QString, QJsonObject> m_attachmentImports;
    QHash<QString, QString> m_attachmentImportStatus;
    QSet<QString> m_attachmentImportRetry;
    QString m_attachmentImportActive;
    // Both projections use the same identified-row diff; account rows need only
    // rowData, with their id as the identity and no predecessor presentation.
    ConversationListModel m_accountModel, m_conversationModel, m_messageModel;
    struct MailViewSnapshot {
        QVariantMap scope, summary;
        QVariantList rows, categories;
        qint64 lifecycle = 0;
        int nextOffset = -1;
        QString nextCursor;
    };
    struct ReaderSnapshot { QVariantMap data; qint64 lifecycle = 0; };
    QCache<QString, MailViewSnapshot> m_mailViewCache{8 * 1024 * 1024};
    QCache<QPair<QString, QString>, ReaderSnapshot> m_readerCache{8 * 1024 * 1024};
    QCache<QString, ReaderSnapshot> m_sourceCache{8 * 1024 * 1024};
    mutable QCache<QByteArray, QVariantList> m_bodyCache{4 * 1024 * 1024};
    mutable HtmlContent *m_htmlContent = nullptr;
    bool m_initialAccountsLoaded = false, m_initialPreferencesLoaded = false;
    QTimer m_notificationTimer;
    QVariantList m_reminders;
    QSet<QString> m_remindersPresented;
    QString m_notificationConfig;
    QString m_mailNotificationConfig;
    QDateTime m_notificationsSince;
    qint64 m_mailNotificationCursor = -1;
    int m_notificationGeneration = 0;
    int m_mailNotificationGeneration = 0;
    bool m_notificationPolling = false;
    bool m_mailNotificationPolling = false;
    QString m_profile, m_engine, m_token, m_account, m_query, m_selected, m_error, m_category;
    QString m_folder = "inbox", m_weekStart = "2026-09-07", m_fixtureToday = "2026-09-11";
    QString m_draftStatus;
    QVariantMap m_googleAuth;
    QVariantMap m_googleAuthConfiguration;
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
    void restoreRecipientInputs(const QVariantMap &draft);
    bool rejectRecipient(const QString &message);
    QString m_recipientError;
    QJsonObject m_draftCreation;
    QString m_draftCreationAccount;
    QString m_draftCreationEmail;
    QVariantMap m_senderCard;
    int m_senderCardGeneration = 0;
    QVariantList m_accounts, m_conversations, m_events, m_todayEvents, m_categories;
    // All-day rows of the calendar anchor's week; they size the header band.
    QVariantList m_calendarWeekAllDay;
    QVariantList m_allAccounts;
    QVariantMap m_accountLifecycleUi;
    QJsonObject m_accountRemovalConfirmation;
    QJsonObject m_archiveDeletionConfirmation;
    QJsonObject m_deferredAccountRemovalPreview;
    int m_accountViewRestorationGeneration = -1;
    QHash<QString, QJsonObject> m_accountRemovalRequests;
    QSet<QString> m_accountRemovalBusy;
    QHash<QString, qint64> m_accountLifecycleRevisions;
    bool m_unsavedAccountBuffer = false;
    QString m_unsavedAccountEmail;
    QString m_archiveSelection;
    quint64 m_draftSaveGeneration = 0;
    std::function<void()> m_afterAccountDisposition;
    QVariantMap m_conversation, m_draft, m_views;
    QVariantMap m_preferences, m_preferencesPending, m_diagnostics, m_filters, m_querySummary;
    QStringList m_collapsedGroups;
    QVariantMap m_searchTerms;
    QString m_searchError, m_searchPeopleQuery;
    QVariantList m_searchPeople;
    int m_searchPeopleGeneration = 0;
    void readSearchResult(const QJsonObject &result, bool ok);
    QString mailMonthZone() const;
    void convertLegacyFilters();
    QVariantMap m_pinnedRow;
    int m_pinnedIndex = -1;
    QVariantList m_calendarSources, m_captures;
    QString m_calendarSearchQuery, m_calendarSearchContext, m_calendarSearchStatus;
    QVariantMap m_calendarSearchResult;
    bool m_calendarSearchActive = false, m_calendarSearchCombined = true, m_calendarSearchBusy = false;
    int m_calendarSearchGeneration = 0;
    QTimer m_calendarSearchTimer;
    QString m_preferencesStatus, m_captureStatus, m_nextCursor;
    int m_captureNextOffset = -1, m_captureGeneration = 0;
    QString m_calendarView = "week", m_calendarDate, m_timelineDate, m_calendarCoverage;
    QString m_timelineCoverage, m_calendarDisplayKey, m_timelineScope;
    QString m_timelineCacheContext;
    QVariantMap m_timelineDayEvents;
    // Raw engine windows by request key; valid until the calendar generation changes.
    QHash<QString, QJsonObject> m_calendarWindows;
    QStringList m_calendarWindowOrder;
    QSet<QString> m_calendarWindowRequests;
    quint64 m_calendarCacheEpoch = 0;
    QTimer m_calendarPrefetchTimer;
    // View and date are saved quietly after navigation settles.
    QVariantMap m_navigationPending;
    QTimer m_navigationSaveTimer;
    QString m_clockDayKey;
    double m_clockHour = -1;
    mutable QString m_clockZoneId;
    mutable QTimeZone m_clockZone;
    mutable bool m_clockZoneCached = false;
    bool m_quietCalendarRead = false;
    int m_timelineGeneration = 0;
    bool m_calendarReady = false, m_timelineReady = false;
    QVariantList m_calendarOperations;
    QHash<QString, QJsonObject> m_calendarOperationCursors;
    QString m_calendarOperationStatus;
    bool m_calendarOperationsLoading = false;
    int m_calendarOperationsGeneration = 0;
    qint64 m_preferencesRevision = 0;
    qint64 m_calendarRevision = -1;
    quint64 m_calendarAuthorityEpoch = 0;
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
    // Private entry trace for the native benchmark; no public state or control.
    quint64 m_accountSelectionRequests = 0;
    QString m_lastAccountSelectionRequest;
    bool m_lastAccountSelectionDirty = false, m_lastAccountSelectionBusy = false;
    int m_desiredConversationCount = 50;
    int m_launchAttempts = 0, m_launchDelay = 1000;
    QList<qint64> m_launchedPids;
    bool m_connected = false, m_loading = false;
    bool m_liveProfileRequested = false, m_syntheticProfile = true;
    bool m_listRefreshPending = false, m_browsingOlder = false;
    QString m_listError;
    qint64 m_mailRevision = -1;
    bool m_automaticReadScope = false;
    bool m_mutatingConversation = false;
    int m_mailScopeCount = 0;
    QString m_mailSelectionStatus, m_bulkAccount;
    qint64 m_bulkLifecycle = 0;
    QList<QJsonObject> m_bulkCommands;
    int m_bulkIndex = 0;
    void runMailSelection();
    bool m_draftDirty = false, m_savingDraft = false, m_shutdown = false;
    bool m_draftConflict = false, m_resolvingDraft = false;
    bool m_resumeSearch = false;
    bool m_shutdownEmitted = false;
    QHash<QString, QJsonObject> m_draftTransferIntents;
    QHash<QString, QString> m_draftTransferMessages;
    QString m_draftTransferActive;
    QJsonObject m_draftTransferDestination;
    bool m_draftTransferCleanupBusy = false;
    void startDraftTransfer(const QString &key);
    void openDraftInAccount(const QString &account, const QString &draft);
    std::function<void()> m_afterDraftSave;
    std::function<void()> m_afterConversationMutation;
    QSet<QString> m_viewSaving, m_viewDirty;
    // Remote search: session-only switch, rows for exactly one account/folder/query.
    bool remoteSearch() const { return m_remoteSearch; }
    void setRemoteSearch(bool value);
    QVariantMap remoteSearchState() const;
    void scheduleRemoteSearch();
    void runRemoteSearch();
    QVariantList withRemoteRows(QVariantList rows) const;
    void loadRemoteThread();
    bool m_remoteSearch = false, m_remoteBusy = false;
    QString m_remoteAccount, m_remoteFolder, m_remoteQuery, m_remoteError;
    QVariantList m_remoteRows;
    int m_remoteSerial = 0;
    QTimer m_remoteSearchTimer;
    // Spam is listed live, a page of Gmail messages at a time, one row per thread.
    void loadSpamRows(bool append);
    void refreshBinCounts();
    void finishBinAction(const QString &action, const QString &account, const QString &folder, const QString &id, const QJsonObject &result, bool ok);
    QVariantMap binCounts() const { return m_binCounts; }
    bool binBusy() const { return m_binBusy; }
    QVariantMap m_binCounts;
    QString m_spamToken;
    bool m_binBusy = false;
};
