#include "MailBackend.h"
#include <QDate>
#include <QJsonArray>
#include <QTimeZone>

namespace {
QTimeZone viewZone() { return QTimeZone("America/Toronto"); }
bool changesCalendarQuery(const QVariantMap &before, const QVariantMap &after) {
    for (const auto &key : {"week_start", "selected_calendars", "calendar_view", "calendar_date", "timeline_date"})
        if (before.value(key) != after.value(key)) return true;
    return before.isEmpty();
}
}

QDateTime MailBackend::clockNow() const {
    return m_fixtureClock ? QDateTime::fromString("2026-09-11T10:42:00-04:00", Qt::ISODate).toTimeZone(viewZone())
                          : QDateTime::currentDateTimeUtc().toTimeZone(viewZone());
}
QString MailBackend::todayDate() const { return clockNow().date().toString(Qt::ISODate); }
QString MailBackend::calendarDateTime(const QString &date, const QString &time, const QString &zone) const {
    const auto day = QDate::fromString(date, Qt::ISODate);
    const auto clock = QTime::fromString(time, "HH:mm");
    const QTimeZone tz(zone.toUtf8());
    if (!day.isValid() || !clock.isValid() || !tz.isValid()) return {};
    const QDateTime value(day, clock, tz);
    if (!value.isValid() || value.date() != day || value.time() != clock) return {};
    return value.toString(Qt::ISODate);
}
double MailBackend::nowHour() const { const auto t = clockNow().time(); return t.hour() + t.minute() / 60.0; }
int MailBackend::calendarDays() const { return m_calendarView == "day" ? 1 : m_calendarView == "week" ? 7 : 42; }
QString MailBackend::calendarFrom() const {
    auto day = QDate::fromString(m_calendarDate, Qt::ISODate);
    if (m_calendarView == "day" || m_calendarView == "agenda") return day.toString(Qt::ISODate);
    if (m_calendarView == "month") day = QDate(day.year(), day.month(), 1);
    const int first = m_preferences.value("week_start", "monday").toString() == "sunday" ? 7 : 1;
    return day.addDays(-((day.dayOfWeek() - first + 7) % 7)).toString(Qt::ISODate);
}
void MailBackend::setCalendarView(const QString &view) {
    if (view != "day" && view != "week" && view != "month" && view != "agenda") return;
    if (view == m_calendarView) return;
    m_calendarView = view; setPreferences({{"calendar_view", view}}); loadCalendar(); emit eventsChanged();
}
void MailBackend::setCalendarDate(const QString &date) {
    if (!QDate::fromString(date, Qt::ISODate).isValid() || date == m_calendarDate) return;
    m_calendarDate = date; setPreferences({{"calendar_date", date}}); loadCalendar(); emit eventsChanged();
}
void MailBackend::moveCalendar(int delta) {
    auto day = QDate::fromString(m_calendarDate, Qt::ISODate);
    day = m_calendarView == "month" ? day.addMonths(delta) : day.addDays(delta * (m_calendarView == "week" ? 7 : m_calendarView == "agenda" ? 42 : 1));
    setCalendarDate(day.toString(Qt::ISODate));
}
void MailBackend::showCalendarToday() { setCalendarDate(todayDate()); }
void MailBackend::moveTimeline(int delta) {
    m_timelineDate = QDate::fromString(m_timelineDate, Qt::ISODate).addDays(delta).toString(Qt::ISODate);
    setPreferences({{"timeline_date", m_timelineDate}}); loadCalendar(); emit eventsChanged();
}
void MailBackend::showTimelineToday() { m_timelineDate = todayDate(); setPreferences({{"timeline_date", m_timelineDate}}); loadCalendar(); emit eventsChanged(); }
void MailBackend::loadCalendarSources() {
    request("calendar_sources", {}, {}, [this](auto result, bool ok) {
        if (!ok) return;
        m_calendarSources = result["calendars"].toArray().toVariantList(); emit eventsChanged();
    });
}
void MailBackend::calendarWrite(const QVariantMap &values) {
    auto params = QJsonObject::fromVariantMap(values);
    const auto account = params.take("account").toString();
    if (account.isEmpty()) { emit calendarWriteFinished(false, {}, "Choose a writable calendar account."); return; }
    if (!params.contains("request_id")) params["request_id"] = mutation()["request_id"];
    ++m_calendarWrites;
    request("calendar_write", account, params, [this](auto result, bool ok) {
        --m_calendarWrites;
        if (!ok) cancelShutdown();
        emit calendarWriteFinished(ok, result.toVariantMap(), ok ? QString{} : result["message"].toString("Calendar change could not be saved."));
        if (ok) { loadCalendar(); loadCalendarSources(); }
        if (m_shutdown) finishShutdown();
    });
}

void MailBackend::loadPreferences() {
    request("preferences_get", {}, {}, [this](auto result, bool ok) {
        if (!ok || m_preferencesSaving) return;
        m_preferencesRevision = result["revision"].toInteger();
        auto preferences = result["preferences"].toObject().toVariantMap();
        for (auto it = m_preferencesPending.cbegin(); it != m_preferencesPending.cend(); ++it) preferences[it.key()] = it.value();
        if (preferences != m_preferences) {
            const bool reloadCalendar = changesCalendarQuery(m_preferences, preferences);
            m_preferences = preferences;
            m_calendarView = preferences.value("calendar_view", "week").toString();
            m_calendarDate = preferences.value("calendar_date").isNull() ? todayDate() : preferences.value("calendar_date").toString();
            m_timelineDate = preferences.value("timeline_date").isNull() ? todayDate() : preferences.value("timeline_date").toString();
            emit preferencesChanged(); if (reloadCalendar) loadCalendar();
        }
    });
}
void MailBackend::setPreferences(const QVariantMap &patch) {
    if (patch.isEmpty() && m_preferencesPending.isEmpty()) return;
    for (auto it = patch.cbegin(); it != patch.cend(); ++it) { m_preferences[it.key()] = it.value(); m_preferencesPending[it.key()] = it.value(); }
    m_preferencesStatus = "Saving…"; emit preferencesChanged(); savePreferences();
}
void MailBackend::savePreferences() {
    if (m_preferencesSaving || m_preferencesPending.isEmpty()) return;
    if (!m_connected) { m_preferencesStatus = "Not saved — reconnect and retry."; cancelShutdown(); emit preferencesChanged(); return; }
    const auto submitted = m_preferencesPending;
    auto params = mutation(); params["preferences"] = QJsonObject::fromVariantMap(submitted); params["expected_revision"] = m_preferencesRevision;
    m_preferencesSaving = true;
    request("preferences_save", {}, params, [this, submitted](auto result, bool ok) {
        m_preferencesSaving = false;
        if (!ok) {
            m_preferencesStatus = "Not saved — " + result["message"].toString("retry after reconnecting");
            cancelShutdown(); emit preferencesChanged();
            // Refresh the optimistic base while keeping the user's pending edits.
            // Retrying then merges them with the other client's committed values.
            if (result["code"].toString() == "stale_revision") loadPreferences();
            return;
        }
        m_preferencesRevision = result["revision"].toInteger();
        for (auto it = submitted.cbegin(); it != submitted.cend(); ++it)
            if (m_preferencesPending.value(it.key()) == it.value()) m_preferencesPending.remove(it.key());
        m_preferencesStatus = m_preferencesPending.isEmpty() ? "Saved on this machine" : "Saving…";
        emit preferencesChanged();
        for (const auto &key : {"week_start", "selected_calendars"})
            if (submitted.contains(key)) { loadCalendar(); break; }
        if (!m_preferencesPending.isEmpty()) savePreferences();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::updateAccountPreferences(const QString &account, const QString &name, const QString &signature) {
    qint64 revision = -1;
    for (const auto &row : m_accounts) if (row.toMap()["id"].toString() == account) revision = row.toMap()["metadata_revision"].toLongLong();
    if (revision < 0) return;
    auto params = mutation(); params["expected_revision"] = revision; params["name"] = name; params["signature_text"] = signature;
    m_preferencesStatus = "Saving…"; emit preferencesChanged();
    ++m_accountWrites;
    request("account_update", account, params, [this](auto result, bool ok) {
        --m_accountWrites;
        if (!ok) cancelShutdown();
        m_preferencesStatus = ok ? "Saved on this machine — existing replies keep their saved signature" : "Not saved — " + result["message"].toString();
        emit preferencesChanged();
        if (ok || result["code"].toString() == "stale_revision") loadAccounts();
        if (ok) loadCalendarSources();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::setCalendarSelected(const QString &account, const QString &calendar, bool selected) {
    QVariantList values;
    const auto current = m_preferences.value("selected_calendars");
    if (current.isNull() || !current.isValid()) {
        for (const auto &source : m_calendarSources) { const auto row = source.toMap(); values.append(QVariantMap{{"account", row["account"]}, {"calendar_id", row["calendar_id"]}}); }
    } else values = current.toList();
    QVariantList next; bool found = false;
    for (const auto &source : values) {
        const auto row = source.toMap(); const bool match = row["account"].toString() == account && row["calendar_id"].toString() == calendar;
        if (match) found = true;
        if (!match || selected) next.append(row);
    }
    if (selected && !found) next.append(QVariantMap{{"account", account}, {"calendar_id", calendar}});
    setPreferences({{"selected_calendars", next}});
}

bool MailBackend::validFilterSender(const QString &sender) const {
    if (sender.toUtf8().size() > 256) return false;
    for (const auto character : sender)
        if (character.category() == QChar::Other_Control) return false;
    return true;
}
void MailBackend::applyFilters(const QVariantMap &filters) {
    if (filters == m_filters) return;
    m_queryActivity.start();
    m_filters = filters; m_pinnedRow.clear(); m_pinnedIndex = -1; remember("filters", filters); ++m_listGeneration; emit queryChanged(); loadConversations();
}
void MailBackend::openConversation(const QString &id) { selectConversationInternal(id, true); }
void MailBackend::clearSenderCard() {
    ++m_senderCardGeneration;
    if (m_senderCard.isEmpty()) return;
    m_senderCard.clear(); emit senderCardChanged();
}
void MailBackend::requestSenderCard(const QString &email) {
    const auto address = email.trimmed().toLower();
    if (!m_connected || m_account.isEmpty() || address.isEmpty() || !validFilterSender(address)) {
        clearSenderCard(); return;
    }
    const auto account = m_account;
    const int generation = ++m_senderCardGeneration;
    m_senderCard = {{"account", account}, {"email", address}, {"loading", true}, {"conversations", QVariantList{}}};
    emit senderCardChanged();
    request("sender_summary", account, {{"email", address}}, [this, account, address, generation](auto result, bool ok) {
        if (account != m_account || generation != m_senderCardGeneration) return;
        m_senderCard = ok ? result.toVariantMap() : QVariantMap{{"account", account}, {"email", address}, {"error", result["message"].toString("Could not load this sender.")}};
        m_senderCard["loading"] = false; emit senderCardChanged();
    });
}
void MailBackend::doneConversation() { mutateConversation("flag", {{"read", true}, {"starred", false}}); }
void MailBackend::editFormattedDraft(const QString &body, const QString &html) {
    if (draftLocked() || m_draft.isEmpty() || (m_draft["body"].toString() == body && m_draft["body_html"].toString() == html)) return;
    m_draft["body"] = body; m_draft["body_html"] = html;
    m_draftDirty = true; m_draftStatus = "Saving…"; emit draftChanged(); m_draftTimer.start();
}
void MailBackend::setDraftAttachment(const QString &id, bool selected) {
    if (draftLocked() || m_draft.isEmpty()) return;
    auto ids = m_draft["attachment_ids"].toList();
    if (selected && !ids.contains(id)) ids.append(id);
    if (!selected) ids.removeAll(id);
    if (ids == m_draft["attachment_ids"].toList()) return;
    m_draft["attachment_ids"] = ids; m_draftDirty = true; m_draftStatus = "Saving…";
    emit draftChanged(); m_draftTimer.start();
}
void MailBackend::setDraftSignature(const QString &signature) {
    if (draftLocked() || m_draft.isEmpty() || m_draft["signature_text"].toString() == signature) return;
    m_draft["signature_text"] = signature; m_draftDirty = true; m_draftStatus = "Saving…";
    emit draftChanged(); m_draftTimer.start();
}
void MailBackend::reloadDraft() {
    if (draftBusy() || m_draft.isEmpty()) return;
    m_draftTimer.stop(); m_resolvingDraft = true;
    m_draftStatus = "Loading the saved draft…"; emit draftChanged();
    const auto account = m_account, id = draftContext();
    request("draft_get", account, draftSelector(), [this, account, id](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account || id != draftContext()) return;
        if (ok) {
            m_draft = result.toVariantMap(); m_recipientInputs.clear(); m_draftDirty = false; m_draftConflict = false;
            m_afterDraftSave = {}; m_draftStatus = "Saved on this machine";
        } else { m_draftStatus = "Not saved — could not reload. Your version remains here."; cancelShutdown(); }
        emit draftChanged();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::saveMyDraftVersion() {
    // Called only after the user confirms replacement of the other saved
    // version. Fetch its revision, retaining every authored field locally, then
    // use the normal optimistic save. A concurrent edit can conflict again.
    if (!m_draftConflict || draftBusy() || m_draft.isEmpty()) return;
    m_draftTimer.stop(); clearError(); m_resolvingDraft = true;
    m_draftStatus = "Checking the current saved revision…"; emit draftChanged();
    const auto account = m_account, id = draftContext();
    request("draft_get", account, draftSelector(), [this, account, id](auto result, bool ok) {
        m_resolvingDraft = false;
        if (account != m_account || id != draftContext()) return;
        if (!ok) { m_draftStatus = "Not saved — could not check the current revision. Your version remains here."; cancelShutdown(); emit draftChanged(); return; }
        // Another client may have made the first legacy save against a newer
        // member. Authored-field replacement cannot retarget frozen context.
        for (const auto &key : {"draft_id", "conversation_id", "mode", "source_message_id", "source_hash"}) {
            if (m_draft.value(key).toString() != result[key].toString()) {
                m_draftStatus = "Not saved — the saved draft has a different original message or composition identity. Your version remains here. Reload the saved draft only to replace these edits.";
                cancelShutdown(); emit draftChanged(); return;
            }
        }
        m_draft["revision"] = result["revision"].toVariant();
        m_draftConflict = false; m_draftDirty = true; m_draftStatus = "Saving your version…"; emit draftChanged();
        saveDraft();
    });
}
void MailBackend::discardDraft() {
    if (m_draft.isEmpty() || draftBusy()) return;
    m_draftTimer.stop(); m_discarding = true; emit draftChanged();
    auto params = draftSelector(); params["request_id"] = mutation()["request_id"]; params["expected_revision"] = m_draft["revision"].toLongLong();
    request("draft_discard", m_account, params, [this](auto result, bool ok) {
        m_discarding = false;
        if (ok) { m_draft = result.toVariantMap(); m_recipientInputs.clear(); m_draftDirty = false; m_draftConflict = false; m_draftStatus = "Draft discarded"; remember("replyOpen", false); remember("selectedDraft", ""); loadAccounts(); if (m_folder == "drafts") loadDraftRows(false); }
        else { m_draftStatus = "Discard failed — your reply remains here"; cancelShutdown(); }
        emit draftChanged();
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::loadCaptures(bool append) {
    if (!m_connected || m_account.isEmpty()) return;
    if (append && m_captureNextOffset < 0) return;
    const auto account = m_account;
    const int generation = ++m_captureGeneration;
    request("capture_history", account, {{"limit", 100}, {"offset", append ? m_captureNextOffset : 0}}, [this, account, append, generation](auto result, bool ok) {
        if (!ok || account != m_account || generation != m_captureGeneration) return;
        const auto previous = m_captures;
        if (!append) m_captures.clear();
        for (const auto &value : result["captures"].toArray()) {
            const auto row = value.toObject().toVariantMap();
            bool exists = false;
            for (const auto &old : m_captures) if (old.toMap()["operation_id"] == row["operation_id"]) { exists = true; break; }
            if (!exists) m_captures.append(row);
        }
        if (!append) for (const auto &value : previous) {
            bool exists = false;
            for (const auto &current : m_captures) if (current.toMap()["operation_id"] == value.toMap()["operation_id"]) { exists = true; break; }
            if (!exists) m_captures.append(value);
        }
        m_captureNextOffset = result["total"].toInteger() > m_captures.size() ? int(m_captures.size()) : -1;
        emit capturesChanged();
    });
}
void MailBackend::loadMoreCaptures() { loadCaptures(true); }
void MailBackend::inspectCapture(const QString &operationId) {
    if (draftBusy()) return;
    QVariantMap record;
    for (const auto &value : m_captures) if (value.toMap()["operation_id"].toString() == operationId) { record = value.toMap(); break; }
    if (record.isEmpty()) return;
    m_capturing = true; emit draftChanged();
    const auto account = m_account;
    QJsonObject params{{"request_id", operationId},
        {"expected_revision", record["draft_revision"].toLongLong()}, {"outcome", record.value("synthetic_outcome", "accepted").toString()}};
    if (record["draft_id"].toString().isEmpty()) params["conversation_id"] = record["conversation_id"].toString();
    else params["draft_id"] = record["draft_id"].toString();
    request("draft_capture", account, params, [this, account, operationId](auto result, bool ok) {
        m_capturing = false;
        if (ok) m_captureStatus = "Local capture " + operationId + ": " + result["state"].toString() + ". The original operation was reused; no email was delivered.";
        else { m_captureStatus = "Could not inspect the recorded operation: " + result["message"].toString(); cancelShutdown(); }
        if (ok) for (auto &value : m_captures) {
            auto row = value.toMap();
            if (row["operation_id"].toString() == operationId) { row["state"] = result["state"].toVariant(); value = row; }
        }
        emit capturesChanged(); emit draftChanged(); loadCaptures();
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}
void MailBackend::captureDraft() {
    if (draftLocked() || m_draft.isEmpty()) return;
    if (m_draftDirty || m_savingDraft) { m_afterDraftSave = [this] { captureDraft(); }; saveDraft(); return; }
    if (m_captureRequest.isEmpty() || m_captureAccount != m_account || m_captureConversation != draftContext()) {
        // The journal survives GUI restart. Resolve unfinished operations before
        // creating another identity, including records beyond normal history pages.
        m_capturing = true; m_captureStatus = "Checking existing local captures…"; emit draftChanged(); emit capturesChanged();
        const auto account = m_account, id = draftContext();
        auto selector = draftSelector();
        const auto draftId = m_draft.value("draft_id", m_selected).toString();
        auto history = QJsonObject{{"draft_id", draftId}, {"unresolved_only", true}, {"limit", 1}, {"offset", 0}};
        request("capture_history", account, history, [this, account, id, selector](auto result, bool ok) {
            m_capturing = false;
            if (!ok || account != m_account || id != draftContext()) {
                m_captureStatus = "Could not check existing captures. No new capture was created. Reconnect and retry.";
                cancelShutdown(); emit draftChanged(); emit capturesChanged(); return;
            }
            const auto unresolved = result["captures"].toArray();
            if (!unresolved.isEmpty()) {
                const auto record = unresolved.first().toObject();
                m_captureRequest = {{"request_id", record["operation_id"]},
                    {"expected_revision", record["draft_revision"]}, {"outcome", record["synthetic_outcome"]}};
                m_captureRequest["draft_id"] = record["draft_id"];
            } else {
                m_captureRequest = mutation();
                m_captureRequest["expected_revision"] = m_draft["revision"].toLongLong();
                for (auto it = selector.begin(); it != selector.end(); ++it) m_captureRequest[it.key()] = it.value();
            }
            m_captureAccount = account; m_captureConversation = id;
            captureDraft();
        });
        return;
    }
    m_capturing = true; m_captureStatus = "Capturing locally…"; emit capturesChanged(); emit draftChanged();
    const auto captureRevision = m_captureRequest["expected_revision"].toInteger();
    const bool earlierRevision = captureRevision != m_draft["revision"].toLongLong();
    request("draft_capture", m_captureAccount, m_captureRequest, [this, captureRevision, earlierRevision](auto result, bool ok) {
        m_capturing = false;
        if (ok) {
            const auto state = result["state"].toString("recorded");
            const bool uncertain = state == "acceptance_unknown" || state == "in_flight";
            m_captureStatus = "Local capture: " + state + " — no email was delivered";
            if (earlierRevision) m_captureStatus += QString(". Checked earlier saved revision %1; your current draft was not captured or changed.").arg(captureRevision);
            if (uncertain) m_captureStatus += ". Retry checks the original saved revision and operation; it does not submit another copy.";
            else m_captureRequest = {};
        } else {
            m_captureStatus = "Capture not confirmed. Retry keeps the same operation ID; your saved draft is retained.";
            cancelShutdown();
            if (result["code"].toString() == "stale_revision" || result["code"].toString() == "invalid_request") m_captureRequest = {};
        }
        emit capturesChanged(); emit draftChanged(); loadCaptures();
        if (ok) saveDraft();
        if (m_shutdown) finishShutdown();
    });
}
