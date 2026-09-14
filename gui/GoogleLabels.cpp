#include "MailBackend.h"
#include <QJsonArray>
#include <QUuid>

QVariantMap MailBackend::googleLabelState() const {
    auto state = m_googleLabelCatalog;
    state["account"] = m_googleLabelsAccount;
    state["loading"] = m_googleLabelsLoading;
    state["refreshing"] = m_googleLabelsRefreshing.contains(m_googleLabelsAccount);
    state["binding"] = m_googleLabelsBinding.contains(m_googleLabelsAccount);
    state["pending"] = m_googleLabelPendingBindings.contains(m_googleLabelsAccount);
    state["status"] = m_googleLabelStatus.value(m_googleLabelsAccount);
    return state;
}

void MailBackend::loadGoogleLabels(const QString &account, bool append) {
    if (m_syntheticProfile || account.isEmpty()) return;
    if (account != m_googleLabelsAccount) {
        m_googleLabelsAccount = account; m_googleLabels.clear(); m_googleLabelCatalog.clear();
        m_googleLabelsLoading = false; ++m_googleLabelsGeneration; append = false;
    }
    if (!m_connected) { m_googleLabelStatus[account] = "Reconnect to inspect cached Google labels."; emit googleLabelsChanged(); return; }
    if (m_googleLabelsLoading) return;
    QJsonObject params{{"limit", 100}};
    if (append) {
        const auto after = m_googleLabelCatalog.value("next_after").toString();
        if (after.isEmpty()) return;
        params["after"] = after;
        params["expected_catalog_revision"] = m_googleLabelCatalog.value("catalog_revision").toLongLong();
    }
    const int generation = ++m_googleLabelsGeneration;
    m_googleLabelsLoading = true; emit googleLabelsChanged();
    request("google_labels", account, params, [this, account, append, generation](auto result, bool ok) {
        if (account != m_googleLabelsAccount || generation != m_googleLabelsGeneration) return;
        m_googleLabelsLoading = false;
        if (!ok) {
            m_googleLabelStatus[account] = result["message"].toString("Cached labels are unavailable. Reconnect and try again.");
            emit googleLabelsChanged();
            if (append && result["code"].toString() == "stale_revision") {
                m_googleLabelStatus[account] = "The label catalog changed. Review the refreshed cached list before linking.";
                loadGoogleLabels(account, false);
            }
            return;
        }
        const auto rows = result.take("labels").toArray().toVariantList();
        if (append) {
            // A revision fence gives one snapshot; identity merging also keeps a
            // repeated explicit page load from duplicating options.
            for (const auto &entry : rows) {
                const auto id = entry.toMap()["id"].toString();
                bool found = false;
                for (auto &existing : m_googleLabels) if (existing.toMap()["id"].toString() == id) { existing = entry; found = true; break; }
                if (!found) m_googleLabels.append(entry);
            }
        } else m_googleLabels = rows;
        m_googleLabelCatalog = result.toVariantMap(); emit googleLabelsChanged();
    });
}

void MailBackend::refreshGoogleLabels(const QString &account) {
    if (m_syntheticProfile || account.isEmpty() || m_googleLabelsRefreshing.contains(account)) return;
    if (!m_connected || m_shutdown || !accountCapability(account, "mail_read") || !googleSyncState(account).value("active").toBool()) {
        m_googleLabelStatus[account] = "Start sync for this Google account before refreshing its labels."; emit googleLabelsChanged(); return;
    }
    m_googleLabelsRefreshing.insert(account); m_googleLabelStatus[account] = "Refreshing Google labels…"; emit googleLabelsChanged();
    googleOperation("google_labels_refresh", account, {}, [this, account](auto result, bool ok) {
        m_googleLabelsRefreshing.remove(account);
        m_googleLabelStatus[account] = ok ? "Google labels refreshed." : result["message"].toString("Google labels could not be refreshed. The previous cache is retained.");
        emit googleLabelsChanged();
        if (ok && account == m_googleLabelsAccount && !m_shutdown) {
            // Supersede an older cached page before reading the newly published
            // catalog. A late page cannot restore the prior generation.
            ++m_googleLabelsGeneration; m_googleLabelsLoading = false;
            loadGoogleLabels(account, false);
        }
    });
}

void MailBackend::bindGoogleLabel(const QString &account, const QString &labelId, const QString &categoryId,
                                const QString &createName, const QVariant &expectedProviderLabelId, qint64 expectedCatalogRevision) {
    if (m_syntheticProfile || m_googleLabelPendingBindings.contains(account) || m_googleLabelsBinding.contains(account)) return;
    const auto reject = [this, account](const QString &message) { m_googleLabelStatus[account] = message; emit googleLabelsChanged(); };
    if (!m_connected || m_shutdown || account != m_account || account != m_googleLabelsAccount || !accountCapability(account, "mail_write")) {
        reject("Choose this Google account with its known mail editing permission."); return;
    }
    if (!m_googleLabelCatalog.value("available").toBool() || expectedCatalogRevision != m_googleLabelCatalog.value("catalog_revision").toLongLong()) {
        reject("Load and review this account's current cached label catalog before linking."); return;
    }
    bool selectable = false;
    for (const auto &entry : m_googleLabels) {
        const auto row = entry.toMap();
        if (row["id"].toString() == labelId && row["present"].toBool() && row["selectable"].toBool()
            && (row["category_id"].isNull() || row["category_id"].toString().isEmpty() || row["category_id"].toString() == categoryId)) { selectable = true; break; }
    }
    if (!selectable) { reject("Choose an available user-created Google label that is not linked elsewhere."); return; }
    auto params = mutation();
    params["provider_label_id"] = labelId; params["expected_catalog_revision"] = expectedCatalogRevision;
    if (categoryId.isEmpty()) {
        if (createName.trimmed().isEmpty()) { reject("Enter a name for the new local label."); return; }
        params["category_id"] = "local-" + QUuid::createUuid().toString(QUuid::WithoutBraces);
        params["create_name"] = createName.trimmed();
    } else {
        bool current = false;
        for (const auto &entry : m_categories) {
            const auto category = entry.toMap();
            if (category["id"].toString() == categoryId && category.value("provider_label_id").toString() == expectedProviderLabelId.toString()) { current = true; break; }
        }
        if (!current) { reject("The local label mapping changed. Review the current labels before linking."); loadCategories(); return; }
        params["category_id"] = categoryId;
        params["expected_provider_label_id"] = expectedProviderLabelId.isNull() || !expectedProviderLabelId.isValid() || expectedProviderLabelId.toString().isEmpty()
            ? QJsonValue(QJsonValue::Null) : QJsonValue(expectedProviderLabelId.toString());
    }
    m_googleLabelPendingBindings.insert(account, params);
    submitGoogleLabelBinding(account);
}

void MailBackend::retryGoogleLabelBinding(const QString &account) { submitGoogleLabelBinding(account); }

void MailBackend::submitGoogleLabelBinding(const QString &account) {
    if (!m_connected || m_shutdown || !m_googleLabelPendingBindings.contains(account) || m_googleLabelsBinding.contains(account)) return;
    const auto params = m_googleLabelPendingBindings.value(account);
    const auto requestId = params["request_id"].toString();
    m_googleLabelsBinding.insert(account); ++m_accountWrites;
    m_googleLabelStatus[account] = "Saving the local label link…"; emit googleLabelsChanged();
    request("google_category_bind", account, params, [this, account, params, requestId](auto result, bool ok) {
        --m_accountWrites; m_googleLabelsBinding.remove(account);
        if (ok && (result["request_id"].toString() != requestId
            || result["category"].toObject()["id"] != params["category_id"]
            || result["category"].toObject()["provider_label_id"] != params["provider_label_id"])) {
            ok = false; result = {{"code", "invalid_response"}, {"message", "The label response did not match this link. Retry checks the same recorded request."}};
        }
        const auto code = result["code"].toString();
        const bool definite = QStringList{"invalid_request", "stale_revision", "label_catalog_required", "label_unavailable", "label_already_mapped",
            "capability_unavailable", "already_exists", "not_found", "profile_kind_mismatch", "request_id_conflict"}.contains(code);
        if (ok || definite) m_googleLabelPendingBindings.remove(account);
        if (ok) m_googleLabelStatus[account] = "Linked locally. Your Google labels were not changed.";
        else {
            m_googleLabelStatus[account] = result["message"].toString("The local link is not confirmed. Retry checks the same recorded request.");
            cancelShutdown();
        }
        result["account"] = account; result["request_id"] = requestId;
        emit googleLabelsChanged();
        emit googleLabelBindingFinished(account, ok, result.toVariantMap(), ok ? QString{} : m_googleLabelStatus.value(account));
        if ((ok || code == "stale_revision") && account == m_account) loadCategories();
        if ((ok || code == "stale_revision") && account == m_googleLabelsAccount) {
            ++m_googleLabelsGeneration; m_googleLabelsLoading = false;
            loadGoogleLabels(account, false);
        }
        if (m_shutdown) finishShutdown();
    });
}
