#pragma once
#include "MailBackend.h"
#include <QPointer>
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
    bool clickItem(QQuickItem *item);
    QObject *filterPopup() const;
    bool filterFeedback(const QString &message) const;
    bool filterResultMatches() const;
    void holdInFlightResponse();
    bool resultMatches() const;
    bool memberIsReadable() const;
    void restore();
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
    int m_stage = 0, m_case = 0, m_ticks = 0, m_stable = 0, m_lastStage = -1;
    bool m_loadingSeen = false, m_searchPending = false, m_originalSettings = false;
};
