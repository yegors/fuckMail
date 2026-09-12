#pragma once
#include "MailBackend.h"
#include <QJsonArray>
#include <QPointer>
#include <QProcess>
#include <QQuickItem>
#include <QQuickWindow>
#include <QTimer>
#include <functional>

// Uses the fixed synthetic source messages through the actual native search UI.
class GuiMailNavigationSelfTest final : public QObject {
public:
    GuiMailNavigationSelfTest(MailBackend &backend, QQuickWindow *window,
        std::function<void(const QString &)> finished, QObject *parent);
private:
    void step();
    void finish(const QString &error = {});
    bool typeQuery(const QString &query);
    bool typeField(const QString &name, const QString &text);
    bool choose(const QString &name, int index);
    bool click(const QString &name);
    bool clickItem(QQuickItem *item, Qt::MouseButton button = Qt::LeftButton);
    bool hoverSender();
    QObject *senderPopup() const;
    bool openAdvancedFilters();
    QObject *filterPopup() const;
    bool filterFeedback(const QString &message) const;
    bool filterResultMatches() const;
    void holdInFlightResponse();
    void observeArrivalReceipt();
    void arrivalCli(const QStringList &arguments, std::function<void(const QJsonObject &)> callback);
    QJsonObject arrivalReport() const;
    bool arrivalMembersMatch(const QJsonObject &conversation, bool starred, bool includeArrival) const;
    bool arrivalCountsMatch() const;
    bool resultMatches() const;
    bool memberIsReadable() const;
    void restore();
    void beginQueueCheck(bool fairness);
    MailBackend &m_backend;
    QQuickWindow *m_window;
    std::function<void(const QString &)> m_finished;
    QTimer m_timer;
    QPointer<QQuickItem> m_originalFocus;
    QVariantMap m_originalDraft, m_originalView, m_originalFilters;
    QString m_originalAccount, m_originalSelected, m_originalQuery, m_originalFolder, m_originalCategory;
    QString m_originalWorkspace;
    QString m_expectedQuery;
    QVariantList m_unfilteredRows;
    QVariantMap m_expectedFilters;
    MailBackend::Callback m_heldDelegate;
    QJsonObject m_heldResult;
    QString m_heldRequestId;
    QString m_obsoleteRequestId, m_finalRequestId;
    QJsonObject m_finalResult;
    int m_heldGeneration = -1;
    bool m_holdArmed = false, m_responseWrapped = false, m_responseHeld = false, m_heldOk = false;
    bool m_obsoleteCanceled = false, m_obsoleteSent = false, m_finalReceived = false, m_finalOk = false;
    QProcess m_arrivalCli;
    QTimer m_arrivalCliTimeout;
    QString m_arrivalReportPath, m_arrivalAccount, m_arrivalThread, m_arrivalId;
    QJsonArray m_arrivalOriginalIds;
    QJsonObject m_arrivalDraft, m_arrivalReceipt, m_arrivalExpectedCounts;
    bool m_arrivalDoneArmed = false, m_arrivalReceiptSeen = false, m_arrivalReceiptOk = false;
    int m_stage = 0, m_case = 0, m_ticks = 0, m_stable = 0, m_lastStage = -1;
    bool m_loadingSeen = false, m_searchPending = false, m_originalSettings = false;
    bool m_surfacesDone = false;
    QString m_surfaceGroup, m_surfaceFirstId, m_senderDraftId;
    QJsonObject m_surfaceCounts, m_senderBefore;
    QStringList m_queueExpected, m_queueObserved;
    bool m_queueCheckActive = false;
};
