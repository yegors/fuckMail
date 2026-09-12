#include "GuiSettingsSelfTest.h"
#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QInputMethodEvent>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QQuickItem>
#include <QPointer>
#include <QDebug>

namespace {
QQuickItem *findVisible(QQuickItem *parent, const QString &name) {
    if (parent->objectName() == name && parent->isVisible()) return parent;
    for (auto child : parent->childItems()) if (auto found = findVisible(child, name)) return found;
    return nullptr;
}
void key(QObject *target, int value) {
    QKeyEvent press(QEvent::KeyPress, value, Qt::NoModifier), release(QEvent::KeyRelease, value, Qt::NoModifier);
    QCoreApplication::sendEvent(target, &press); QCoreApplication::sendEvent(target, &release);
}
void reveal(QQuickItem *field) {
    for (auto ancestor = field->parentItem(); ancestor; ancestor = ancestor->parentItem()) {
        if (!ancestor->property("contentY").isValid() || !ancestor->property("contentHeight").isValid()) continue;
        const auto rect = field->mapRectToItem(ancestor, field->boundingRect());
        const double current = ancestor->property("contentY").toDouble();
        const double maximum = qMax(0.0, ancestor->property("contentHeight").toDouble() - ancestor->height());
        if (rect.top() < 4) ancestor->setProperty("contentY", qBound(0.0, current + rect.top() - 4, maximum));
        else if (rect.bottom() > ancestor->height() - 4) ancestor->setProperty("contentY", qBound(0.0, current + rect.bottom() - ancestor->height() + 4, maximum));
    }
}
QJsonObject authoredFields(const QJsonObject &draft) {
    QJsonObject result;
    for (const auto &field : {"body", "body_html", "signature_text", "attachment_ids", "from_name", "from", "to", "subject", "identity_revision"})
        result[field] = draft[field];
    return result;
}
}

GuiSettingsSelfTest::GuiSettingsSelfTest(MailBackend &backend, QQuickWindow *window, QString profile, QString engine,
    std::function<void(const QVariantMap &, const QString &)> finished, QObject *parent)
    : QObject(parent), m_backend(backend), m_window(window), m_profile(std::move(profile)), m_engine(std::move(engine)), m_finished(std::move(finished)) {
    m_timer.setInterval(80);
    connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    m_timer.start();
}
QQuickItem *GuiSettingsSelfTest::item(const QString &name) const { return findVisible(m_window->contentItem(), name); }
bool GuiSettingsSelfTest::click(const QString &name) {
    auto control = item(name);
    if (!control || !control->isEnabled()) return false;
    auto target = control;
    if (control->property("textDocument").isValid()) {
        for (auto ancestor = control->parentItem(); ancestor; ancestor = ancestor->parentItem())
            if (ancestor->clip()) { target = ancestor; break; }
    }
    reveal(target);
    auto visible = control->mapRectToScene(control->boundingRect());
    for (auto ancestor = control->parentItem(); ancestor; ancestor = ancestor->parentItem())
        if (ancestor->clip()) visible = visible.intersected(ancestor->mapRectToScene(ancestor->boundingRect()));
    visible = visible.intersected(QRectF(0,0,m_window->width(),m_window->height()));
    if (visible.isEmpty()) return false;
    const auto position = visible.center();
    const QPointF global = m_window->mapToGlobal(position.toPoint());
    QMouseEvent press(QEvent::MouseButtonPress, position, global, Qt::LeftButton, Qt::LeftButton, Qt::NoModifier);
    QMouseEvent release(QEvent::MouseButtonRelease, position, global, Qt::LeftButton, Qt::NoButton, Qt::NoModifier);
    QCoreApplication::sendEvent(m_window, &press); QCoreApplication::sendEvent(m_window, &release);
    return true;
}
bool GuiSettingsSelfTest::type(const QString &name, const QString &text) {
    auto field = item(name);
    if (!field || !field->isEnabled()) return false;
    reveal(field); field->forceActiveFocus(); QMetaObject::invokeMethod(field, "selectAll");
    if (text.isEmpty()) key(field, Qt::Key_Backspace);
    else { QInputMethodEvent input; input.setCommitString(text); QCoreApplication::sendEvent(field, &input); }
    return field->property("text").toString() == text;
}
bool GuiSettingsSelfTest::choose(const QString &name, int index) {
    auto control = item(name);
    if (!control || !control->isEnabled()) return false;
    reveal(control); control->forceActiveFocus();
    const auto delta = index - control->property("currentIndex").toInt();
    for (int i = 0; i < qAbs(delta); ++i) key(control, delta > 0 ? Qt::Key_Down : Qt::Key_Up);
    return control->property("currentIndex").toInt() == index;
}
bool GuiSettingsSelfTest::saved() const { return !m_backend.preferencesPending() && m_backend.preferencesStatus().startsWith("Saved"); }
bool GuiSettingsSelfTest::draftIntact() const { return m_backend.selectedId() == m_id && authoredFields(QJsonObject::fromVariantMap(m_backend.draft())) == m_draftSnapshot; }
void GuiSettingsSelfTest::finish(const QString &error) {
    if (!m_finished) return;
    m_timer.stop(); m_backend.setProperty("selfTestExpectedError", false);
    auto callback = std::move(m_finished); m_finished = {};
    const auto result = m_result;
    deleteLater();
    callback(result, error); // No access to this after handing control to the owner.
}
void GuiSettingsSelfTest::cli(const QStringList &arguments, std::function<void(const QJsonObject &)> callback) {
    m_timer.stop(); m_cli.disconnect(this);
    connect(&m_cli, &QProcess::finished, this, [this, callback](int code, QProcess::ExitStatus status) {
        const auto reply = QJsonDocument::fromJson(m_cli.readAllStandardOutput()).object();
        if (code != 0 || status != QProcess::NormalExit || !reply["ok"].toBool()) { finish("Settings CLI readback failed: " + QString::fromUtf8(m_cli.readAllStandardError())); return; }
        QPointer<GuiSettingsSelfTest> guard(this);
        callback(reply["result"].toObject()); if (guard && m_finished) m_timer.start();
    });
    connect(&m_cli, &QProcess::errorOccurred, this, [this](auto) { finish("Cannot run the independent Settings CLI."); });
    m_cli.start(QDir(QFileInfo(m_engine).absolutePath()).filePath("fm"
#ifdef Q_OS_WIN
        ".exe"
#endif
    ), QStringList{"--profile", m_profile, "--engine", m_engine, "--account", "windscribe", "--json"} + arguments);
}
void GuiSettingsSelfTest::step() {
    if (++m_ticks > 450) { finish(QString("Native Settings timed out at stage %1: %2").arg(m_stage).arg(m_backend.error())); return; }
    if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE") && m_lastStage != m_stage) { qInfo() << "Native Settings stage" << m_stage; m_lastStage = m_stage; }
    if (!m_backend.error().isEmpty() && !m_backend.property("selfTestExpectedError").toBool()) { finish(m_backend.error()); return; }
    switch (m_stage) {
    case 0:
        if (!m_backend.connected() || m_backend.loading() || m_backend.draftBusy() || m_backend.draft().isEmpty()) return;
        if (m_backend.account() != "windscribe") { finish("Settings walkthrough requires the existing Windscribe reply."); return; }
        m_id = m_backend.selectedId(); m_draftSnapshot = authoredFields(QJsonObject::fromVariantMap(m_backend.draft()));
        m_originalName = m_backend.currentAccount()["name"].toString(); m_originalSignature = m_backend.currentAccount()["signature_text"].toString();
        m_window->setProperty("workspace", "mail"); m_window->setProperty("settingsOpen", false);
        m_backend.setQuery(""); m_backend.setCategory(""); m_backend.applyFilters({}); m_backend.setFolder("all");
        QMetaObject::invokeMethod(m_window->findChild<QObject *>("mailWorkspace"), "openReply");
        m_wait.start(); ++m_stage; break;
    case 1: {
        if (m_backend.loading() || m_wait.elapsed() < 240 || !click("replyComposer")) return;
        const auto list=item("conversationList"), reader=item("reader"), editor=item("replyComposer");
        if (!list || !reader || !editor) { finish("Mail scroll surfaces are missing."); return; }
        // Programmatic viewport setup tests restoration, not physical wheel input.
        // Use each Flickable's public property and existing end-of-movement handler.
        m_listY=qMin(240.0,list->property("contentHeight").toDouble()-list->height());
        m_readerY=qMin(160.0,reader->property("contentHeight").toDouble()-reader->height());
        list->setProperty("contentY",m_listY); reader->setProperty("contentY",m_readerY);
        editor->forceActiveFocus(); QMetaObject::invokeMethod(editor, "select", Q_ARG(int,6), Q_ARG(int,10));
        if (!QMetaObject::invokeMethod(list,"movementEnded") || !QMetaObject::invokeMethod(reader,"movementEnded")) { finish("Cannot exercise the viewport persistence handlers."); return; }
        m_wait.restart(); ++m_stage; break;
    }
    case 2: {
        const auto list = item("conversationList"), reader = item("reader"), editor = item("replyComposer");
        if (!list || !reader || !editor || m_wait.elapsed() < 850 || list->property("moving").toBool() || reader->property("moving").toBool()) return;
        if (qEnvironmentVariableIsSet("FM_GUI_TEST_TRACE")) qInfo() << "Settings programmatic viewport setup: list=" << list->property("contentY") << "reader=" << reader->property("contentY") << "editorFocus=" << editor->hasActiveFocus()
            << "active=" << (m_window->activeFocusItem()?m_window->activeFocusItem()->objectName():QString()) << "stored list/reader=" << m_backend.presentation()["listY"] << m_backend.presentation()["readerY"];
        if (m_listY <= 0 || m_readerY <= 0 || qAbs(m_listY-m_readerY)<1 || !editor->hasActiveFocus()
            || qAbs(list->property("contentY").toDouble()-m_listY)>1 || qAbs(reader->property("contentY").toDouble()-m_readerY)>1
            || qAbs(m_backend.presentation()["listY"].toDouble()-m_listY)>1 || qAbs(m_backend.presentation()["readerY"].toDouble()-m_readerY)>1) {
            finish(QString("Programmatic viewport setup failed: list=%1 (extent=%2, viewport=%3), reader=%4 (extent=%5, viewport=%6), editorFocus=%7, active=%8.")
                .arg(list->property("contentY").toDouble()).arg(list->property("contentHeight").toDouble()).arg(list->height()).arg(reader->property("contentY").toDouble()).arg(reader->property("contentHeight").toDouble())
                .arg(reader->height()).arg(editor->hasActiveFocus()).arg(m_window->activeFocusItem()?m_window->activeFocusItem()->objectName():QString())); return;
        }
        ++m_stage;
        cli({"call","view_get"},[this](auto result) {
            const auto view=result["view"].toObject();
            if (qAbs(view["listY"].toDouble()-m_listY)>1 || qAbs(view["readerY"].toDouble()-m_readerY)>1) { finish("The independent CLI did not observe both persisted viewport positions before Settings."); return; }
            if (!click("settingsButton")) finish("The Settings button is unavailable after viewport setup.");
        });
        break;
    }
    case 3:
        if (!click("settingsPage_Accounts")) return;
        ++m_stage; break;
    case 4:
        if (!type("accountName_windscribe", "") || !click("saveAccount_windscribe")) return;
        ++m_stage; break;
    case 5: {
        auto status = item("settingsStatus");
        if (!status || status->property("text").toString() != "Use a display name between 1 and 80 characters." || item("accountName_windscribe")->property("text").toString() != "") { finish("Empty name validation did not retain the native field and visible error."); return; }
        ++m_stage;
        cli({"accounts"}, [this](auto result) {
            for (const auto &value : result["accounts"].toArray()) if (value.toObject()["id"].toString() == "windscribe") {
                if (value.toObject()["name"].toString() != m_originalName) finish("Rejected empty name changed persisted account metadata."); return;
            }
            finish("Independent account readback omitted Windscribe.");
        });
        break;
    }
    case 6:
        if (!type("accountName_windscribe", m_name) || !click("saveAccount_windscribe")) return;
        ++m_stage; break;
    case 7:
        if (!saved() || m_backend.currentAccount()["name"].toString() != m_name) return;
        if (!draftIntact() || !item("accountTab_windscribe")->property("text").toString().startsWith(m_name)) { finish("Account display-name save changed the existing draft or failed to update its tab."); return; }
        if (!click("settingsPage_Mail")) return;
        ++m_stage; break;
    case 8:
        if (!type("signature_windscribe", m_oversizedSignature)) return;
        m_backend.setProperty("selfTestExpectedError", true);
        if (!click("saveSignature_windscribe")) return;
        ++m_stage; break;
    case 9:
        if (!m_backend.preferencesStatus().startsWith("Not saved")) return;
        if (m_backend.error() != "Signature must be text of at most 16 KiB" || !item("signature_windscribe")
            || item("signature_windscribe")->property("text").toString() != m_oversizedSignature || m_backend.preferencesPending() || !draftIntact()) { finish("Rejected signature lost retained edits, changed the authored draft or offered the wrong retry domain."); return; }
        m_backend.clearError(); m_backend.setProperty("selfTestExpectedError", false); ++m_stage;
        cli({"accounts"}, [this](auto result) {
            for (const auto &value : result["accounts"].toArray()) if (value.toObject()["id"].toString() == "windscribe") {
                if (value.toObject()["name"].toString() != m_name || value.toObject()["signature_text"].toString() != m_originalSignature) finish("Rejected signature changed persisted metadata."); return;
            }
            finish("Independent metadata readback omitted Windscribe.");
        });
        break;
    case 10:
        if (!type("signature_windscribe", m_signature) || !click("saveSignature_windscribe")) return;
        ++m_stage; break;
    case 11:
        if (!saved() || m_backend.currentAccount()["signature_text"].toString() != m_signature) return;
        if (!draftIntact()) { finish("Changing the default signature changed the existing authored draft."); return; }
        if (!click("settingsPage_Calendar")) return;
        ++m_stage; break;
    case 12:
        if (!choose("weekStartPreference",1)) return;
        ++m_stage; break;
    case 13:
        if (!saved() || m_backend.preferences()["week_start"].toString() != "sunday") return;
        // End selects 23:00 for the start, which must be rejected against 22:00.
        if (auto start = item("calendar_start_hour")) { reveal(start); start->forceActiveFocus(); key(start,Qt::Key_End); }
        else return;
        ++m_stage; break;
    case 14:
        if (!item("settingsStatus") || item("settingsStatus")->property("text").toString() != "The end of the visible range must be later than its start."
            || m_backend.preferences()["calendar_start_hour"].toInt() != 7 || item("calendar_start_hour")->property("currentIndex").toInt() != 7) { finish("Invalid visible-hour range was not rejected/restored."); return; }
        if (!choose("calendar_start_hour",8) || !choose("calendar_end_hour",19) || !choose("timeline_start_hour",9) || !choose("timeline_end_hour",17)) { finish("Native hour choices did not reach their requested values."); return; }
        ++m_stage; break;
    case 15:
        if (!saved() || !m_backend.calendarReady() || !m_backend.timelineReady()) return;
        if (m_backend.preferences()["calendar_start_hour"].toInt()!=8 || m_backend.preferences()["calendar_end_hour"].toInt()!=20
            || m_backend.preferences()["timeline_start_hour"].toInt()!=9 || m_backend.preferences()["timeline_end_hour"].toInt()!=18) { finish("Hour preferences did not persist in the adapter."); return; }
        if (!click("hideAllCalendars")) return;
        ++m_stage; break;
    case 16:
        if (!saved() || !m_backend.calendarReady() || !m_backend.timelineReady()) return;
        if (m_backend.preferences()["selected_calendars"].isNull() || !m_backend.preferences()["selected_calendars"].toList().isEmpty()
            || !m_backend.events().isEmpty() || !m_backend.todayEvents().isEmpty()) { finish("Hide all did not produce a truthful empty calendar and timeline."); return; }
        ++m_stage;
        cli({"preferences","show"}, [this](auto result) { const auto chosen=result["preferences"].toObject()["selected_calendars"]; if (!chosen.isArray() || !chosen.toArray().isEmpty()) finish("CLI did not observe explicit empty calendar selection."); });
        break;
    case 17:
        if (!click("calendarSelected_personal_primary")) return;
        ++m_stage; break;
    case 18:
        if (!saved() || !m_backend.calendarReady() || !m_backend.timelineReady()) return;
        if (m_backend.account() != "windscribe" || m_backend.preferences()["selected_calendars"].toList().size()!=1 || m_backend.events().isEmpty()) { finish("Selecting a personal calendar changed mailbox context or failed to expose its events."); return; }
        for (const auto &event : m_backend.events()) if (event.toMap()["account"].toString() != "personal") { finish("Personal-only calendar selection leaked a different source."); return; }
        ++m_stage;
        cli({"preferences","show"}, [this](auto result) { if (result["preferences"].toObject()["selected_calendars"].toArray() != QJsonArray{QJsonObject{{"account","personal"},{"calendar_id","primary"}}}) finish("CLI calendar selection did not preserve the independent source identity."); });
        break;
    case 19:
        if (!click("showAllCalendars")) return;
        ++m_stage; break;
    case 20:
        if (!saved() || !m_backend.calendarReady() || !m_backend.timelineReady()) return;
        if (!m_backend.preferences()["selected_calendars"].isNull() || !draftIntact()) { finish("Restoring all sources changed mailbox draft identity."); return; }
        m_result = {{"account", "windscribe"}, {"name", m_name}, {"signature_text", m_signature},
            {"draft_snapshot", m_draftSnapshot.toVariantMap()},
            {"preferences", QVariantMap{{"density","compact"},{"week_start","sunday"},{"calendar_start_hour",8},{"calendar_end_hour",20},
                {"timeline_start_hour",9},{"timeline_end_hour",18},{"selected_calendars",QVariant()}}}};
        ++m_stage;
        cli({"preferences","show"}, [this](auto result) {
            const auto expected=QJsonObject::fromVariantMap(m_result["preferences"].toMap()), actual=result["preferences"].toObject();
            for (auto it=expected.begin();it!=expected.end();++it) if (actual[it.key()]!=it.value()) { finish("CLI differs from the native Settings preference: "+it.key()); return; }
        });
        break;
    case 21:
        ++m_stage;
        cli({"accounts"}, [this](auto result) {
            for (const auto &value : result["accounts"].toArray()) if (value.toObject()["id"].toString()=="windscribe") {
                if (value.toObject()["name"].toString()!=m_name || value.toObject()["signature_text"].toString()!=m_signature) finish("CLI did not observe the corrected account name/signature."); return;
            }
            finish("CLI omitted the updated account.");
        });
        break;
    case 22:
        ++m_stage;
        cli({"draft","show",m_id}, [this](auto result) { if (authoredFields(result)!=m_draftSnapshot) finish("Settings changed the independently persisted authored draft snapshot."); });
        break;
    case 23:
        if (!click("settingsDoneButton")) return;
        m_wait.restart(); ++m_stage; break;
    case 24: {
        if (m_wait.elapsed()<240) return;
        const auto list=item("conversationList"), reader=item("reader"), editor=item("replyComposer");
        if (!list || !reader || !editor || !draftIntact() || !editor->hasActiveFocus() || editor->property("selectionStart").toInt()!=6 || editor->property("selectionEnd").toInt()!=10
            || qAbs(list->property("contentY").toDouble()-m_listY)>1 || qAbs(reader->property("contentY").toDouble()-m_readerY)>1) {
            finish(QString("Settings did not restore independent Mail scroll and editor focus/selection (list %1/%2, reader %3/%4).")
                .arg(list?list->property("contentY").toDouble():-1).arg(m_listY).arg(reader?reader->property("contentY").toDouble():-1).arg(m_readerY)); return;
        }
        qInfo() << "Native Settings passed: validation retention, account metadata, unchanged authored draft, independent calendar preferences and Mail viewport/focus restoration (programmatic viewport setup; physical wheel input unverified).";
        finish(); break;
    }
    }
}
