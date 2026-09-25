#include <QCoreApplication>
#include "CalendarLayout.h"
#include "FmIconItem.h"
#include <QJsonObject>
#include <QDebug>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFile>
#include <QDir>
#include <QDate>
#include <QDateTime>
#include <QTimeZone>
#include <QGuiApplication>
#include <QMouseEvent>
#include <QFont>
#include <QFontDatabase>
#include <QJSValue>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickItem>
#include <QQuickWindow>
#include <QQuickStyle>
#include <QTemporaryDir>
#include <cstdlib>
#include <functional>
#include <source_location>

static void require(bool condition, const char *message) {
    if (!condition) { qCritical() << message; std::exit(1); }
}
static QVariant plain(QVariant value) {
    return value.canConvert<QJSValue>() ? value.value<QJSValue>().toVariant() : value;
}
static void settle(const std::function<bool()> &predicate,
                   const std::source_location &caller = std::source_location::current()) {
    QElapsedTimer timer; timer.start();
    while (!predicate() && timer.elapsed() < 1500) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    if (!predicate()) {
        qCritical() << "QML state did not settle at" << caller.file_name() << caller.line()
                    << caller.function_name();
        std::exit(1);
    }
}

// The actual QML controls run against scripted completions. No engine, vault,
// provider connection or real account is involved in this boundary test.
class CalendarBackendProbe : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool syntheticProfile MEMBER syntheticProfile NOTIFY modeChanged)
    Q_PROPERTY(QString account MEMBER account NOTIFY accountChanged)
    Q_PROPERTY(bool connected MEMBER connected NOTIFY connectedChanged)
    Q_PROPERTY(QVariantList accounts MEMBER accounts NOTIFY accountsChanged)
    Q_PROPERTY(QVariantMap googleSync MEMBER googleSync NOTIFY googleSyncChanged)
    Q_PROPERTY(QVariantList calendarSources MEMBER calendarSources NOTIFY eventsChanged)
    Q_PROPERTY(QVariantList events MEMBER events NOTIFY eventsChanged)
    Q_PROPERTY(QVariantList todayEvents MEMBER todayEvents NOTIFY eventsChanged)
    Q_PROPERTY(QVariantMap preferences MEMBER preferences NOTIFY eventsChanged)
    Q_PROPERTY(bool calendarReady MEMBER calendarReady CONSTANT)
    Q_PROPERTY(bool timelineReady MEMBER calendarReady CONSTANT)
    Q_PROPERTY(QString todayDate MEMBER todayDate CONSTANT)
    Q_PROPERTY(QString calendarDate MEMBER calendarDate NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarFrom MEMBER calendarFrom NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarView MEMBER calendarView NOTIFY eventsChanged)
    Q_PROPERTY(QString calendarCoverage MEMBER calendarCoverage CONSTANT)
    Q_PROPERTY(QVariantMap calendarMailActivity READ calendarMailActivity NOTIFY eventsChanged)
    Q_PROPERTY(int calendarBandRows MEMBER calendarBandRows NOTIFY eventsChanged)
    Q_PROPERTY(double nowHour MEMBER nowHour CONSTANT)
    Q_PROPERTY(QVariantList calendarOperations MEMBER calendarOperations NOTIFY calendarOperationsChanged)
    Q_PROPERTY(bool calendarOperationsLoading MEMBER calendarOperationsLoading NOTIFY calendarOperationsChanged)
    Q_PROPERTY(bool calendarOperationsHasMore MEMBER calendarOperationsHasMore NOTIFY calendarOperationsChanged)
    Q_PROPERTY(QString calendarOperationStatus MEMBER calendarOperationStatus NOTIFY calendarOperationsChanged)
    Q_PROPERTY(QVariantList categories MEMBER categories NOTIFY categoriesChanged)
    Q_PROPERTY(QVariantList googleLabels MEMBER googleLabels NOTIFY googleLabelsChanged)
    Q_PROPERTY(QVariantMap googleLabelState MEMBER googleLabelState NOTIFY googleLabelsChanged)
    Q_PROPERTY(QVariantList reminders MEMBER reminders CONSTANT)
public:
    bool mailCombined = false, mailActive = false;
    Q_INVOKABLE void setCalendarMailActivityContext(bool active, bool combined) { mailActive = active; if (mailCombined != combined) { mailCombined = combined; emit eventsChanged(); } }
    QVariantMap calendarMailActivity() const {
        QVariantList days;
        const auto first = QDate::fromString(calendarFrom, Qt::ISODate);
        for (int i = 0; i < 42; ++i) days.append(QVariantMap{{"date",first.addDays(i).toString(Qt::ISODate)}, {"received",mailCombined ? 24 : 7}, {"sent",mailCombined ? 6 : 2}});
        return {{"state","ready"},{"days",days},{"time_zone","America/Toronto"},{"cached_only",true}};
    }
    bool syntheticProfile = false, connected = true, capability = true, readCapability = false, active = true;
    bool calendarReady = true, calendarOperationsLoading = false, calendarOperationsHasMore = false;
    double nowHour = 8;
    QString todayDate = "2026-09-13", calendarOperationStatus;
    QString calendarDate = "2026-09-13", calendarFrom = "2026-09-13", calendarView = "week", calendarCoverage;
    int calendarBandRows = 0;
    QString account = "alpha";
    QVariantList accounts{QVariantMap{{"id", "alpha"}}, QVariantMap{{"id", "beta"}}};
    QVariantMap googleSync{{"alpha", QVariantMap{{"active", true}}}};
    QVariantMap preferences{{"calendar_display_time_zone", "America/Toronto"}, {"default_event_duration_minutes", 60}};
    QVariantList calendarSources{
        QVariantMap{{"account", "alpha"}, {"calendar_id", "primary"}, {"account_name", "Alpha"}, {"name", "Personal"}, {"writable", true}, {"can_create", true}},
        QVariantMap{{"account", "beta"}, {"calendar_id", "primary"}, {"account_name", "Beta"}, {"name", "Workspace"}, {"writable", true}, {"can_create", true}}};
    QVariantList events, todayEvents, calendarOperations;
    QVariantList categories{QVariantMap{{"id", "work"}, {"name", "Work"}, {"provider_label_id", "Label_old"}}};
    QVariantList googleLabels{
        QVariantMap{{"id", "INBOX"}, {"name", "Inbox"}, {"present", true}, {"selectable", false}},
        QVariantMap{{"id", "Label_new"}, {"name", "New label"}, {"present", true}, {"selectable", true}},
        QVariantMap{{"id", "Label_gone"}, {"present", false}, {"selectable", false}},
        QVariantMap{{"id", "Label_taken"}, {"name", "Taken"}, {"present", true}, {"selectable", true}, {"category_id", "other"}}};
    QVariantMap googleLabelState{{"account", "alpha"}, {"catalog_revision", 1}, {"available", true}, {"connection_state", "connected"}, {"stale_connection", false}, {"next_after", "Label_new"}};
    QVariantMap lastWrite, lastAction, signaledResponse;
    QObject *responseControl = nullptr;
    Q_SLOT void captureResponseSignal() { signaledResponse = responseControl ? plain(responseControl->property("pendingResponse")).toMap() : QVariantMap{}; }
    int writes = 0, actions = 0, refreshes = 0;
    int occurrenceReads = 0;
    QVariantMap lastOccurrenceSelector;
    QString lastOccurrenceCorrelation;
    int occurrenceEpoch = 1;
    int labelCacheReads = 0, labelRefreshes = 0, labelBindings = 0;
    QVariantMap lastLabelBinding, lastLabelRead;
    Q_INVOKABLE bool accountCapability(const QString &account, const QString &name) const { return (capability || (readCapability && name == "calendar_read")) && account == "alpha" && (name == "calendar_read" || name == "calendar_write" || name == "mail_read" || name == "mail_write"); }
    Q_INVOKABLE QVariantMap googleSyncState(const QString &account) const { return {{"active", active && account == "alpha"}}; }
    Q_INVOKABLE QString calendarDateTime(const QString &date, const QString &time, const QString &) const { return date + "T" + time + ":00-04:00"; }
    Q_INVOKABLE QVariantList calendarPreview(const QVariantMap &event) const { return calendarPresentation({QJsonObject::fromVariantMap(event)}, QDate::fromString(calendarFrom, Qt::ISODate), calendarView == "day" ? 1 : 7, QTimeZone("America/Toronto")); }
    Q_INVOKABLE void calendarWrite(const QVariantMap &value) { lastWrite = value; ++writes; }
    Q_INVOKABLE QString queueCalendarDeletion(const QVariantMap &value, const QVariantMap &) { lastWrite = value; ++writes; return {}; }
    Q_INVOKABLE QString queueCalendarDecline(const QVariantMap &value, const QVariantMap &) { lastWrite = value; ++writes; return {}; }
    Q_INVOKABLE QVariantMap calendarOccurrenceContext(const QString &account, const QString &calendar) const {
        return capability && active && connected && account == "alpha" && calendar == "primary"
            ? QVariantMap{{"account", account}, {"calendar_id", calendar}, {"epoch", occurrenceEpoch}} : QVariantMap{};
    }
    Q_INVOKABLE void readCalendarOccurrence(const QVariantMap &selector, const QString &correlation) {
        lastOccurrenceSelector = selector; lastOccurrenceCorrelation = correlation; ++occurrenceReads;
    }
    Q_INVOKABLE void calendarOperationAction(const QString &account, const QString &id, const QString &action, qint64 revision) {
        lastAction = {{"account", account}, {"request_id", id}, {"action", action}, {"revision", revision}}; ++actions;
    }
    Q_INVOKABLE void cancelGoogleOperation(const QString &account, const QString &id) { lastAction = {{"account", account}, {"request_id", id}, {"action", "cancel"}}; ++actions; }
    Q_INVOKABLE void refresh() { ++refreshes; }
    Q_INVOKABLE void setPreferences(const QVariantMap &patch) {
        for (auto it = patch.cbegin(); it != patch.cend(); ++it) preferences[it.key()] = it.value();
        emit eventsChanged();
    }
    Q_INVOKABLE void remember(const QString &, const QVariant &) {}
    Q_INVOKABLE void setCalendarView(const QString &view) { calendarView = view; emit eventsChanged(); }
    Q_INVOKABLE void setCalendarDate(const QString &date) { calendarDate = calendarFrom = date; emit eventsChanged(); }
    Q_INVOKABLE void openCalendarWeek(const QString &date) { calendarView = "week"; setCalendarDate(date); }
    Q_INVOKABLE void showCalendarToday() { setCalendarDate(todayDate); }
    Q_INVOKABLE QString calendarDayOf(const QString &start) const { return start.size() == 10 ? start : QDateTime::fromString(start, Qt::ISODate).toTimeZone(QTimeZone("America/Toronto")).date().toString(Qt::ISODate); }
    Q_INVOKABLE void moveCalendar(int) {}
    Q_INVOKABLE void joinCalendarEvent(const QVariantMap &) { ++reminderActions; }
    // The full event view shares the reminder screen; viewing must not touch reminders.
    QVariantList reminders;
    int reminderActions = 0, sounds = 0;
    Q_INVOKABLE void dismissCalendarReminder(const QVariantMap &) { ++reminderActions; }
    Q_INVOKABLE void snoozeCalendarReminder(const QVariantMap &) { ++reminderActions; }
    Q_INVOKABLE void playReminderSound(int) { ++sounds; }
    Q_INVOKABLE void stopReminderSound() {}
    Q_INVOKABLE QString calendarReminderWhen(const QVariantMap &) const { return "Scripted time"; }
    Q_INVOKABLE void loadCalendarOperations() {}
    Q_INVOKABLE void loadMoreCalendarOperations() {}
    Q_INVOKABLE void retryCalendarOperation(const QString &account, const QString &id) { lastAction = {{"account", account}, {"request_id", id}, {"action", "local_retry"}}; ++actions; }
    Q_INVOKABLE void loadGoogleLabels(const QString &account, bool append) { ++labelCacheReads; lastLabelRead = {{"account", account}, {"append", append}}; }
    Q_INVOKABLE void refreshGoogleLabels(const QString &) { ++labelRefreshes; }
    Q_INVOKABLE void bindGoogleLabel(const QString &account, const QString &label, const QString &category, const QString &name, const QVariant &expected, qint64 revision) {
        ++labelBindings; lastLabelBinding = {{"account", account}, {"label", label}, {"category", category}, {"name", name}, {"expected", expected}, {"revision", revision}};
    }
    Q_INVOKABLE void retryGoogleLabelBinding(const QString &) { ++labelBindings; }
    void complete(bool ok, QVariantMap result) { emit calendarWriteFinished(ok, result, ok ? QString{} : "Scripted unconfirmed result."); }
signals:
    void modeChanged();
    void accountChanged();
    void connectedChanged();
    void accountsChanged();
    void googleSyncChanged();
    void eventsChanged();
    void calendarOperationsChanged();
    void categoriesChanged();
    void googleLabelsChanged();
    void googleLabelBindingFinished(const QString &account, bool ok, const QVariantMap &result, const QString &error);
    void calendarWriteFinished(bool ok, const QVariantMap &result, const QString &error);
    void calendarOccurrenceReadFinished(const QString &correlation, bool ok, const QVariantMap &result, const QString &error);
};

static QQuickItem *visualNamed(QQuickItem *item, const QString &name) {
    if (item->objectName() == name) return item;
    for (auto child : item->childItems()) if (auto found = visualNamed(child, name)) return found;
    return nullptr;
}
static QObject *findNamed(QObject *root, const char *name) {
    const auto key = QString::fromUtf8(name);
    if (auto found = root->findChild<QObject *>(key)) return found;
    // ListView owns delegate lifetime separately from its visual parent. Those
    // dynamically incubated controls need not be QObject descendants of root.
    if (auto window = qobject_cast<QQuickWindow *>(root)) return visualNamed(window->contentItem(), key);
    return nullptr;
}
static QObject *named(QObject *root, const char *name) {
    auto found = findNamed(root, name);
    require(found, name); return found;
}
static void call(QObject *object, const char *method) { require(QMetaObject::invokeMethod(object, method), method); }
static void call(QObject *object, const char *method, const QVariant &arg) { require(QMetaObject::invokeMethod(object, method, Q_ARG(QVariant, arg)), method); }
static void call(QObject *object, const char *method, const QVariant &first, const QVariant &second) { require(QMetaObject::invokeMethod(object, method, Q_ARG(QVariant, first), Q_ARG(QVariant, second)), method); }
static QVariantMap event() {
    return {{"id", "alpha:primary:event"}, {"account", "alpha"}, {"account_name", "Alpha"}, {"calendar_id", "primary"}, {"calendar_name", "Personal"},
        {"event_id", "event"}, {"etag", "v1"}, {"title", "Original title"}, {"start", "2026-09-13T09:00:00-04:00"}, {"end", "2026-09-13T10:00:00-04:00"},
        {"date", "2026-09-13"}, {"time_zone", "America/Toronto"}, {"all_day", false}, {"location", "Room"}, {"description", "Provider notes"},
        {"conference_url", "https://example.test/retained"}, {"can_edit", true}, {"can_delete", true}, {"can_rsvp", false}, {"attendees", QVariantList{}}};
}
static QVariantMap resultFor(const QVariantMap &write, const QString &state, int revision = 2) {
    return {{"account", write["account"]}, {"request_id", write["request_id"]}, {"state", state}, {"revision", revision}};
}

int main(int argc, char **argv) {
    QQuickStyle::setStyle("Basic");
    QGuiApplication app(argc, argv);
    for (const auto font : {"Geist-Regular.ttf", "Geist-Medium.ttf", "Geist-SemiBold.ttf"})
        require(QFontDatabase::addApplicationFont(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/fonts/" + font) >= 0, "Load actual bundled UI fonts");
    app.setFont(QFont("Geist", 10));
    QTemporaryDir copy; require(copy.isValid(), "Temporary QML directory");
    require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/CalendarMailCount.qml", copy.filePath("CalendarMailCount.qml")), "Copy mail count component");
    for (const auto name : {"AccountBadge.qml","ShortcutRegistry.qml","FmScrollBar.qml","Theme.qml", "FmDialog.qml", "FmConfirmDialog.qml", "FmDateField.qml", "FmSwitch.qml", "FmSegmented.qml", "FmButton.qml", "FmToolTip.qml", "FmIcon.qml", "FmComboBox.qml", "FmTextField.qml", "CalendarOccurrenceRead.qml", "EventEditor.qml", "EventDetails.qml", "ReminderPopup.qml", "AttendeeList.qml", "SenderAvatar.qml", "CalendarOperations.qml", "GoogleLabelSetup.qml", "CalendarView.qml", "CalendarEventList.qml", "StartupOverlay.qml"})
        require(QFile::copy(QString::fromUtf8(FM_GUI_SOURCE_DIR) + "/" + name, copy.filePath(name)), "Copy actual QML control");
    QFile module(copy.filePath("qmldir")); require(module.open(QIODevice::WriteOnly), "QML singleton directory");
    module.write("singleton Theme 1.0 Theme.qml\nsingleton ShortcutRegistry 1.0 ShortcutRegistry.qml\n"); module.close();
    CalendarBackendProbe backend;
    QQmlApplicationEngine engine; FmIconItem::registerType();
    bool qmlError = false;
    QObject::connect(&engine, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { qmlError = true; for (const auto &error : errors) qWarning() << error.toString(); });
    engine.rootContext()->setContextProperty("backend", &backend);
    engine.loadData(R"QML(import QtQuick
import QtQuick.Controls
ApplicationWindow { width: 1100; height: 850; visible: true
    font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14)
    property alias testTheme: appearance.themeId
    property alias testTextSize: appearance.textSize
    QtObject { id: appearance; property string themeId: "graphite"; property string textSize: "default" }
    Binding { target: Theme; property: "themeId"; value: appearance.themeId }
    Binding { target: Theme; property: "textSize"; value: appearance.textSize }
    color: Theme.canvas
    palette.window: Theme.surface; palette.windowText: Theme.text
    palette.base: Theme.canvas; palette.text: Theme.text
    palette.button: Theme.surface; palette.buttonText: Theme.text
    palette.highlight: Theme.accent; palette.highlightedText: Theme.selectedText
    EventEditor { id: testEditor }
    EventDetails { onEditRequested: function(event) { testEditor.editEvent = event; testEditor.openForDate(event.start.slice(0, 10)) }; exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 80 } } }
    CalendarOperations {}
    GoogleLabelSetup { visible: false; width: 560 }
})QML", QUrl::fromLocalFile(copy.filePath("Harness.qml")));
    require(!engine.rootObjects().isEmpty(), "Actual Calendar QML loads");
    auto root = engine.rootObjects().first();
    auto editor = named(root, "eventEditor"), details = named(root, "eventDetails"), history = named(root, "calendarOperations");
    backend.responseControl = details;
    require(QObject::connect(details, SIGNAL(pendingResponseChanged()), &backend, SLOT(captureResponseSignal())), "Observe exact pending response property signal");
    auto title = named(root, "eventTitle"), save = named(root, "eventSaveButton");
    const auto openEditor = [&](QVariantMap value) { editor->setProperty("editEvent", value); call(editor, "openForDate", "2026-09-13"); settle([&] { return editor->property("opened").toBool(); }); };
    const auto catalog = backend.calendarSources;
    backend.calendarSources.clear(); emit backend.eventsChanged();
    openEditor({}); title->setProperty("text", "Typed while first sync discovers calendars");
    require(!save->property("enabled").toBool(), "An empty catalog cannot authorize creation");
    backend.calendarSources = catalog; emit backend.eventsChanged();
    auto target = named(root, "eventTargetCalendar");
    settle([&] { return save->property("enabled").toBool(); });
    require(editor->property("targetAccount") == "alpha" && target->property("currentIndex").toInt() == 0
        && title->property("text") == "Typed while first sync discovers calendars", "Late catalog enables the open form without losing typing");
    // Guests: once there is a second person the Going? list shows everyone,
    // you first as organizer, and a guest added here reads "Invite on save".
    auto going = qobject_cast<QQuickItem *>(named(root, "eventGoingList"));
    require(going && !going->isVisible(), "A new event alone has no Going? list");
    editor->setProperty("guestAddresses", QVariantList{"new@example.test"});
    require(going->isVisible() && named(root, "editorAttendeeStatus_0")->property("text") == "Accepted"
        && named(root, "editorAttendeeStatus_1")->property("text") == "Invite on save", "Going? lists you as organizer and the unsaved guest");
    editor->setProperty("guestAddresses", QVariantList{});
    require(!going->isVisible(), "Removing the only guest hides Going?");
    backend.calendarSources = {catalog[1], catalog[0]}; emit backend.eventsChanged();
    settle([&] { return target->property("currentIndex").toInt() == 1; });
    require(editor->property("targetAccount") == "alpha", "Catalog reordering preserves the exact chosen target");
    backend.calendarSources = {catalog[1]}; emit backend.eventsChanged();
    settle([&] { return target->property("currentIndex").toInt() == -1; });
    require(!save->property("enabled").toBool() && editor->property("targetAccount") == "alpha"
        && title->property("text") == "Typed while first sync discovers calendars", "Withdrawal disables creation without silently switching accounts or losing fields");
    backend.calendarSources = catalog; emit backend.eventsChanged();
    settle([&] { return save->property("enabled").toBool() && target->property("currentIndex").toInt() == 0; });
    call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });
    const auto original = event();
    backend.events = {original};
    openEditor(original);
    require(editor->property("width").toInt() == 440 && editor->property("x").toInt() == 660, "Prototype editor width and right anchoring");
    auto deleteButton = qobject_cast<QQuickItem *>(named(root, "eventDeleteButton"));
    require(deleteButton && deleteButton->isVisible() && deleteButton->mapToScene({0, 0}).y() < 112, "Delete remains in the editor header");
    call(deleteButton, "clicked");
    auto deleteDialog = named(root, "eventDeleteConfirmation"); settle([&] { return deleteDialog->property("opened").toBool(); });
    require(deleteDialog->property("width").toInt() == 360 && deleteDialog->property("height").toDouble() > 0, "Delete dialog uses the prototype width and wrapped content height");
    call(deleteDialog, "reject");
    require(named(root, "eventAttendees")->property("readOnly").toBool() && named(root, "eventConference")->property("readOnly").toBool(), "Live unsupported fields retain their displayed values");
    title->setProperty("text", "Changed title"); call(editor, "save");
    auto frozen = backend.lastWrite;
    require(backend.writes == 1 && frozen["account"] == "alpha" && frozen["calendar_id"] == "primary", "Live edit freezes original account/calendar");
    require(frozen["fields"].toMap() == QVariantMap{{"title", "Changed title"}}, "Title edit omits unchanged dates, descriptions, attendees and conference fields");
    call(named(root, "eventCancelButton"), "clicked");
    require(backend.lastAction["action"] == "cancel" && backend.lastAction["request_id"] == frozen["request_id"], "Long request cancellation retains exact recorded identity");
    auto unrelated = resultFor(frozen, "acknowledged"); unrelated["account"] = "beta"; backend.complete(true, unrelated);
    require(editor->property("saving").toBool(), "Other account completion cannot close editor");
    unrelated = resultFor(frozen, "acknowledged"); unrelated["request_id"] = "different"; backend.complete(true, unrelated);
    require(editor->property("saving").toBool(), "Other request completion cannot close editor");
    backend.complete(false, resultFor(frozen, "unknown", 4));
    require(editor->property("opened").toBool() && editor->property("unknownOutcome").toBool() && title->property("text") == "Changed title", "Unknown outcome retains form");
    call(editor, "save");
    require(backend.writes == 1 && backend.lastAction["action"] == "reconcile" && backend.lastAction["request_id"] == frozen["request_id"], "Unknown outcome reconciles same request without replay");
    backend.complete(false, resultFor(frozen, "prepared", 5)); call(editor, "save");
    require(backend.lastAction["action"] == "resume" && backend.lastAction["revision"] == 5 && backend.writes == 1, "Prepared unsent change uses explicit resume");
    backend.complete(false, resultFor(frozen, "ready", 6)); call(editor, "save");
    require(backend.lastAction["action"] == "resume" && backend.writes == 1, "Ready resumes immutable recorded intent");
    backend.complete(false, resultFor(frozen, "in_flight", 7)); call(editor, "save");
    require(backend.lastAction["action"] == "check", "In-flight change only checks local status");
    backend.complete(true, resultFor(frozen, "accepted", 8));
    settle([&] { return !editor->property("visible").toBool(); });

    openEditor(original); named(root, "eventEndTime")->setProperty("text", "11:00"); call(editor, "save");
    const auto time = backend.lastWrite["fields"].toMap();
    require(time.size() == 4 && time["start"] == original["start"] && time["end"] == "2026-09-13T11:00:00-04:00" && time.contains("all_day") && time.contains("time_zone"), "Time edits send complete time group only");
    frozen = backend.lastWrite; backend.complete(false, resultFor(frozen, "conflict", 9));
    call(editor, "resolveOperation", "abandon"); require(backend.lastAction["action"] == "abandon" && backend.lastAction["revision"] == 9, "Abandon uses frozen operation revision");
    backend.complete(false, resultFor(frozen, "abandoned", 10));
    require(!editor->property("unknownOutcome").toBool() && editor->property("opened").toBool() && named(root, "eventEndTime")->property("text") == "11:00", "Abandon retains editable field values without claiming save");
    call(editor, "close");

    openEditor(original); backend.active = false; emit backend.googleSyncChanged();
    require(!save->property("enabled").toBool(), "Active sync notifier disables live save");
    const auto beforeGate = backend.writes; call(editor, "save"); require(backend.writes == beforeGate, "Direct save also enforces active sync");
    backend.active = true; backend.capability = false; emit backend.accountsChanged();
    require(!save->property("enabled").toBool(), "Account capability notifier disables live save");
    backend.capability = true; emit backend.accountsChanged();
    auto revoked = original; revoked["can_edit"] = false; revoked["can_delete"] = false; backend.events = {revoked}; emit backend.eventsChanged();
    require(!save->property("enabled").toBool() && !deleteButton->isVisible(), "Open editor observes projected capability revocation");
    require(title->property("text") == "Original title", "Capability refresh retains form contents");
    backend.events = {original}; emit backend.eventsChanged();
    call(editor, "deleteEvent"); frozen = backend.lastWrite;
    require(frozen["kind"] == "delete" && frozen["expected_etag"] == "v1" && !frozen.contains("fields"), "Delete submits scoped etag and no invented field patch");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });

    call(details, "openAt", original, QVariant{}); settle([&] {return details->property("opened").toBool();});
    const auto beforeDirectDelete=backend.writes;
    call(named(root,"eventScopedCancel"),"clicked");
    auto directDeleteDialog=named(root,"eventDetailsDeleteConfirmation");
    settle([&] {return directDeleteDialog->property("opened").toBool();});
    require(backend.writes==beforeDirectDelete&&!editor->property("visible").toBool(),"Details deletion opens only its confirmation before any write");
    call(directDeleteDialog,"reject");require(backend.writes==beforeDirectDelete,"Cancel leaves event unchanged");
    settle([&] {return !directDeleteDialog->property("visible").toBool();});
    call(named(root,"eventScopedCancel"),"clicked");settle([&] {return directDeleteDialog->property("opened").toBool();});
    call(named(root,"confirmDetailsDelete"),"clicked");frozen=backend.lastWrite;
    require(backend.writes==beforeDirectDelete+1&&frozen["kind"]=="delete"&&frozen["event_id"]==original["event_id"]&&frozen["expected_etag"]=="v1","Details delete retains exact event identity and revision");
    settle([&]{return !details->property("visible").toBool();});
    call(details,"deleteEvent",QString{});require(backend.writes==beforeDirectDelete+1,"Closed popup cannot queue a second deletion");

    auto invitation = original; invitation["can_edit"] = false; invitation["can_delete"] = false; invitation["can_rsvp"] = true; invitation["self_response"] = "needsAction";
    invitation["attendees"] = QVariantList{QVariantMap{{"email", "self@example.test"}, {"self", true}}};
    call(details, "openAt", invitation, QVariant{}); settle([&] { return details->property("visible").toBool(); });
    require(!details->property("opened").toBool(), "Exercise the actual 180ms popup enter transition");
    auto acceptResponse = qobject_cast<QQuickItem *>(named(root, "eventRsvp-accepted"));
    require(acceptResponse && acceptResponse->isVisible() && acceptResponse->isEnabled(), "RSVP control accepts input during enter transition");
    call(acceptResponse, "clicked"); frozen = backend.lastWrite;
    settle([&] { return details->property("opened").toBool(); });
    require(details->property("saving").toBool() && plain(details->property("pendingResponse")).toMap()["request_id"] == frozen["request_id"], "Finishing enter transition preserves the already submitted RSVP identity");
    require(frozen["kind"] == "rsvp" && frozen["account"] == "alpha" && frozen["response"] == "accepted", "Self RSVP keeps the event scope");
    auto otherEvent = original; otherEvent["id"] = "beta:primary:other"; otherEvent["account"] = "beta"; otherEvent["event_id"] = "other";
    call(details, "openAt", otherEvent, QVariant{});
    require(plain(details->property("eventData")).toMap()["account"] == "alpha" && plain(details->property("pendingResponse")).toMap()["request_id"] == frozen["request_id"], "Saving RSVP cannot retarget to another event");
    unrelated = resultFor(frozen, "acknowledged"); unrelated["request_id"] = "other"; backend.complete(true, unrelated);
    require(details->property("saving").toBool(), "Other completion cannot acknowledge RSVP");
    backend.complete(false, resultFor(frozen, "unknown", 3)); const auto beforeRsvp = backend.writes; call(details, "respond", "declined");
    require(backend.writes == beforeRsvp && plain(details->property("eventData")).toMap()["self_response"] == "needsAction", "Unknown RSVP cannot be replaced or shown as accepted");
    call(details, "resolveResponse", "reconcile"); require(backend.lastAction["request_id"] == frozen["request_id"], "RSVP reconciliation retains original request");
    backend.complete(true, resultFor(frozen, "acknowledged"));
    require(plain(details->property("eventData")).toMap()["etag"] == "v1" && plain(details->property("eventData")).toMap()["self_response"] == "needsAction", "Live acknowledgment waits for provider projection rather than inventing an etag/response");
    call(details, "respond", "tentative"); frozen = backend.lastWrite;
    backend.complete(false, resultFor(frozen, "unknown", 11));
    const auto pendingWrites = backend.writes, pendingActions = backend.actions;
    call(details, "openAt", otherEvent, QVariant{});
    require(plain(details->property("eventData")).toMap()["account"] == "alpha" && plain(details->property("pendingResponse")).toMap()["request_id"] == frozen["request_id"], "Visible unconfirmed RSVP retains its displayed event and request");
    call(details, "close");
    require(details->property("visible").toBool() && !details->property("opened").toBool(), "Exercise a real popup closing transition");
    call(details, "openAt", otherEvent, QVariant{});
    require(plain(details->property("eventData")).toMap()["account"] == "alpha" && plain(details->property("pendingResponse")).toMap()["request_id"] == frozen["request_id"], "Closing popup cannot retarget its unconfirmed RSVP");
    settle([&] { return !details->property("visible").toBool(); });
    call(details, "openAt", otherEvent, QVariant{}); settle([&] { return details->property("opened").toBool(); });
    require(plain(details->property("eventData")).toMap()["account"] == "beta" && !details->property("hasPendingResponse").toBool(), "Closed pending popup can reopen a different event");
    require(backend.writes == pendingWrites && backend.actions == pendingActions, "Closing and reopening neither resends nor abandons the recorded RSVP; recovery remains in Activity");
    call(details, "close"); settle([&] { return !details->property("visible").toBool(); });

    // View opens the reminder screen for any event: a countdown to its real
    // start, its full notes and Join, but no Snooze, no acknowledgment and no
    // sound on opening. It sounds only when a point passes while it is open.
    auto viewed = original; viewed["description"] = "Agenda first\nDial-in second\nThird line\nFourth line";
    viewed["organizer"] = "rebecca@example.test";
    viewed["attendees"] = QVariantList{
        QVariantMap{{"email", "igor@example.test"}, {"displayName", "Igor"}, {"responseStatus", "tentative"}},
        QVariantMap{{"email", "rebecca@example.test"}, {"displayName", "Rebecca"}, {"organizer", true}, {"responseStatus", "accepted"}},
        QVariantMap{{"email", "room@resource.example.test"}, {"resource", true}, {"responseStatus", "accepted"}},
        QVariantMap{{"email", "me@example.test"}, {"self", true}, {"responseStatus", "needsAction"}},
        QVariantMap{{"email", "aly@example.test"}, {"displayName", "Aly"}, {"responseStatus", "declined"}}};
    const auto viewedStart = QDateTime::currentDateTimeUtc().addSecs(2 * 86400 + 3723);
    viewed["start"] = viewedStart.toString(Qt::ISODate); viewed["end"] = viewedStart.addSecs(3600).toString(Qt::ISODate);
    call(details, "openAt", viewed, QVariant{}); settle([&] { return details->property("opened").toBool(); });
    auto cardNotes = named(root, "eventDetailsNotesText");
    require(qobject_cast<QQuickItem *>(cardNotes)->isVisible() && cardNotes->property("maximumLineCount").toInt() == 3
        && cardNotes->property("text") == viewed["description"], "Card shows the start of the notes in at most three lines");
    require(named(root, "eventEditButton")->property("outlined").toBool() && !named(root, "eventEditButton")->property("prominent").toBool(), "Edit is an outline action beside View");
    call(named(root, "eventViewButton"), "clicked");
    auto fullView = named(root, "eventFullView");
    settle([&] { return fullView->property("opened").toBool() && !details->property("visible").toBool(); });
    auto viewPanel = named(root, "eventFullViewPanel");
    auto viewKicker = named(root, "reminderKicker"), viewCountdown = named(root, "reminderCountdown"), viewClose = named(root, "reminderDismiss");
    require(viewKicker->property("text") == "UPCOMING" && viewCountdown->property("text").toString().startsWith("2d 1:02:")
        && viewCountdown->property("font").value<QFont>().pixelSize() == 80, "Days-away countdown reads Nd h:mm:ss at 80px");
    require(viewClose->property("text") == "Close" && !qobject_cast<QQuickItem *>(named(root, "reminderSnooze"))->isVisible()
        && named(root, "reminderPrimary")->property("text") == "Join meeting" && named(root, "reminderNotesText")->property("notes") == viewed["description"],
        "View offers Join and Close with the full notes, never Snooze");
    require(named(root, "reminderMeetingUrl")->property("text") == "example.test/retained", "The meeting URL row shows the link without https://");
    {
        QStringList statuses;
        for (int i = 0; i < 5; ++i) if (auto status = findNamed(root, QByteArray("reminderAttendeeStatus_" + QByteArray::number(i)).constData())) statuses << status->property("text").toString();
        require(statuses == QStringList{"Accepted", "Maybe", "Declined", "No reply"}, qPrintable("Guests: organizer, guests, then you, without rooms: " + statuses.join(",")));
        if (const auto dir = qEnvironmentVariable("FM_CALENDAR_CAPTURE_DIR"); !dir.isEmpty() && QDir().mkpath(dir)) {
            QElapsedTimer paint; paint.start(); while (paint.elapsed() < 300) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
            qobject_cast<QQuickWindow *>(root)->grabWindow().save(QDir(dir).filePath("event-view.png"));
        }
    }
    require(backend.sounds == 0 && viewPanel->property("chimes").toInt() == 0 && backend.reminderActions == 0, "Opening View is silent and acknowledges nothing");
    const auto viewedStartMs = viewPanel->property("startMs").toDouble();
    viewPanel->setProperty("nowMs", viewedStartMs - 59000);
    require(backend.sounds == 1 && viewKicker->property("text") == "STARTING NOW", "Passing the one-minute point while open sounds once");
    viewPanel->setProperty("nowMs", viewedStartMs + 5000);
    require(backend.sounds == 2 && viewKicker->property("text") == "IN PROGRESS" && viewCountdown->property("text") == "+0:05"
        && named(root, "reminderRelative")->property("text") == "Started 0:05 ago" && !viewPanel->property("alerting").toBool(), "A started viewed event is in progress, counting up without an alarm");
    viewPanel->setProperty("nowMs", viewedStartMs + 3601000);
    require(viewKicker->property("text") == "ENDED" && viewCountdown->property("text") == "Ended" && named(root, "reminderRelative")->property("text") == "This event is over"
        && !qobject_cast<QQuickItem *>(named(root, "reminderPrimary"))->isVisible() && viewClose->property("text") == "Close", "An ended event offers only Close");
    viewPanel->setProperty("nowMs", viewedStartMs - (400. * 86400 + 5) * 1000);
    require(viewCountdown->property("text") == "1y 35d 0:00:05" && viewCountdown->property("font").value<QFont>().pixelSize() == 64, "Years-away countdown reads Ny Nd h:mm:ss at 64px");
    call(viewClose, "clicked"); settle([&] { return !fullView->property("visible").toBool(); });
    require(backend.reminderActions == 0, "Closing View leaves reminders untouched");

    QVariantList rows;
    for (const auto state : {"prepared", "ready", "in_flight", "unknown", "accepted", "acknowledged", "conflict", "rejected", "abandoned"})
        rows.append(QVariantMap{{"request_id", state}, {"account", "alpha"}, {"account_name", "Alpha"}, {"target", QVariantMap{{"calendar", "primary"}}}, {"kind", "create"}, {"state", state}, {"revision", 4}});
    backend.calendarOperations = rows; emit backend.calendarOperationsChanged(); call(history, "open"); settle([&] { return history->property("opened").toBool(); });
    auto operationList = qobject_cast<QQuickItem *>(named(root, "calendarOperationList"));
    require(operationList, "Activity list is a visual control");
    settle([&] { return operationList->property("count").toInt() == 9 && operationList->height() > 0; });
    const auto activityControl = [&](int index, const char *name) {
        call(operationList, "forceLayout");
        require(QMetaObject::invokeMethod(operationList, "positionViewAtIndex", Q_ARG(int, index), Q_ARG(int, 4)), "Position actual activity row in view");
        QQuickItem *control = nullptr;
        settle([&] { control = qobject_cast<QQuickItem *>(findNamed(root, name)); return control && control->isVisible() && control->isEnabled() && control->width() > 0 && control->height() > 0; });
        require(operationList->contains(control->mapToItem(operationList, {control->width() / 2, control->height() / 2})), "Activity action is inside the visible list viewport");
        return control;
    };
    call(activityControl(0, "calendarOperationResolve-prepared"), "clicked");
    require(backend.lastAction["action"] == "resume" && backend.lastAction["request_id"] == "prepared", "Activity recovers prepared intent after restart");
    call(activityControl(3, "calendarOperationResolve-unknown"), "clicked"); require(backend.lastAction["action"] == "reconcile", "Activity reconciles unknown outcome");
    call(history, "close");

    auto labels = named(root, "googleLabelSetup"); labels->setProperty("visible", true);
    require(backend.labelCacheReads > 0 && backend.labelRefreshes == 0 && backend.labelBindings == 0, "Opening label setup queries only local cache");
    require(plain(labels->property("userLabels")).toList().size() == 1, "Missing, system and other-category labels cannot be selected");
    auto chooseGoogle = named(root, "googleLabelsProviderLabel"); chooseGoogle->setProperty("currentIndex", 0);
    require(QMetaObject::invokeMethod(chooseGoogle, "activated", Q_ARG(int, 0)), "Choose Google label");
    auto labelBind = named(root, "googleLabelsBind");
    require(labelBind->property("enabled").toBool(), "Explicit eligible Google label enables local creation");
    backend.active = false; backend.googleLabelState["connection_state"] = "disconnected"; backend.googleLabelState["stale_connection"] = true; emit backend.googleLabelsChanged(); emit backend.googleSyncChanged();
    require(labelBind->property("enabled").toBool() && !named(root, "googleLabelsRefresh")->property("enabled").toBool(), "Known offline cached labels remain locally bindable without provider refresh");
    call(labelBind, "clicked");
    require(backend.labelBindings == 1 && backend.lastLabelBinding["label"] == "Label_new" && backend.lastLabelBinding["account"] == "alpha" && backend.lastLabelBinding["category"].toString().isEmpty(), "Create uses explicit selected label and frozen account");
    auto localChoice = named(root, "googleLabelsLocalCategory"); localChoice->setProperty("currentIndex", 1);
    require(QMetaObject::invokeMethod(localChoice, "activated", Q_ARG(int, 1)), "Choose existing local label");
    chooseGoogle->setProperty("currentIndex", 0); require(QMetaObject::invokeMethod(chooseGoogle, "activated", Q_ARG(int, 0)), "Choose replacement Google label");
    call(labelBind, "clicked");
    auto remap = named(root, "googleLabelsRemapConfirmation"); settle([&] { return remap->property("opened").toBool(); });
    require(backend.labelBindings == 1, "Remap waits for explicit confirmation");
    backend.account = "beta"; emit backend.accountChanged(); call(remap, "accept");
    require(backend.lastLabelBinding["account"] == "alpha" && backend.lastLabelBinding["expected"] == "Label_old" && backend.lastLabelBinding["revision"] == 1, "Remap confirmation retains original account, mapping and revision across account switch");
    require(backend.lastLabelRead["account"] == "beta" && backend.labelRefreshes == 0, "Account switch loads cached labels without provider access");
    emit backend.googleLabelBindingFinished("alpha", true, {}, {});
    require(labels->property("account") == "beta", "Other-account completion cannot replace current label setup");
    labels->setProperty("visible", false); backend.account = "alpha"; emit backend.accountChanged();

    // Calendar16: the native form authors only the removable address set. The
    // shared Core boundary validates it and retains all protected/source data.
    backend.active = true; emit backend.googleSyncChanged();
    auto roster = original;
    roster["can_edit_attendees"] = true;
    roster["guest_addresses"] = QStringList{"alpha@example.test", "beta@example.test"};
    roster["protected_attendees"] = QVariantList{QVariantMap{{"email", "organizer@example.test"}, {"organizer", true}, {"self", true}}};
    roster["attendees"] = QVariantList{QVariantMap{{"email", "organizer@example.test"}, {"organizer", true}, {"self", true}},
        QVariantMap{{"email", "alpha@example.test"}, {"response", "accepted"}}, QVariantMap{{"email", "beta@example.test"}, {"response", "tentative"}}};
    backend.events = {roster};
    openEditor(roster);
    auto attendeeInput = named(root, "eventAttendees");
    require(!attendeeInput->property("readOnly").toBool(), "Complete organizer roster enables native guest input");
    require(named(root, "eventProtectedGuests")->property("text").toString().contains("organizer@example.test"), "Protected organizer/self remains separately visible");
    title->setProperty("text", "Roster-preserving title"); call(save, "clicked"); frozen = backend.lastWrite;
    require(frozen["fields"].toMap() == QVariantMap{{"title", "Roster-preserving title"}}, "Unchanged supported roster is omitted from title edit");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });
    openEditor(roster); attendeeInput->setProperty("text", "new@example.test"); call(attendeeInput, "accepted");
    call(save, "clicked"); frozen = backend.lastWrite;
    const auto editedGuests = frozen["fields"].toMap()["attendees"].toStringList();
    require(editedGuests == QStringList{"alpha@example.test", "beta@example.test", "new@example.test"}
        && frozen["fields"].toMap().size() == 1 && frozen["expected_etag"] == "v1", "Guest-only save has exact removable addresses and frozen version, no copied provider metadata");
    backend.complete(false, resultFor(frozen, "conflict", 17));
    require(plain(editor->property("guestAddresses")).toStringList() == editedGuests && editor->property("staleRevision").toBool(), "Stale guest update preserves complete authored guest set");
    call(editor, "resolveOperation", "abandon"); backend.complete(false, resultFor(frozen, "abandoned", 18));
    auto newer = roster; newer["etag"] = "v2"; backend.events = {newer}; emit backend.eventsChanged();
    call(named(root, "eventUseCurrentVersion"), "clicked"); call(save, "clicked");
    require(backend.lastWrite["expected_etag"] == "v2" && backend.lastWrite["request_id"] != frozen["request_id"]
        && backend.lastWrite["fields"].toMap()["attendees"].toStringList() == editedGuests, "Explicit current-version review retains guest edits and creates a new fenced request");
    frozen = backend.lastWrite; backend.complete(true, resultFor(frozen, "reconciled", 19));
    settle([&] { return !editor->property("visible").toBool(); });
    openEditor(roster);
    call(named(root, "eventRemoveGuest-1"), "clicked"); call(named(root, "eventRemoveGuest-0"), "clicked"); call(save, "clicked");
    frozen = backend.lastWrite;
    require(frozen["fields"].toMap().contains("attendees") && frozen["fields"].toMap()["attendees"].toList().isEmpty(), "Removing all guest chips supplies explicit empty array, never protected people");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });
    openEditor(roster); attendeeInput->setProperty("text", "alpha@example.test, invalid mailbox"); call(save, "clicked"); frozen = backend.lastWrite;
    require(frozen["fields"].toMap()["attendees"].toStringList().size() == 4, "Malformed/duplicate input reaches shared validation without silent deduplication");
    auto refused = resultFor(frozen, "rejected", 20); refused["code"] = "invalid_request"; backend.complete(false, refused);
    require(attendeeInput->property("text") == "alpha@example.test, invalid mailbox", "Refusal retains incomplete native guest input");
    call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });
    auto incomplete = roster; incomplete["can_edit_attendees"] = false; incomplete["guest_addresses"] = QStringList{}; incomplete["attendee_edit_reason"] = "incomplete_attendees";
    openEditor(incomplete); title->setProperty("text", "Allowed title on incomplete roster"); call(save, "clicked"); frozen = backend.lastWrite;
    require(attendeeInput->property("readOnly").toBool() && !frozen["fields"].toMap().contains("attendees"), "Incomplete roster blocks only roster editing, preserving title-only edits");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });
    openEditor(roster); attendeeInput->setProperty("text", "retained@example.test");
    backend.events = {incomplete}; emit backend.eventsChanged(); const int beforeRosterRevoke = backend.writes; call(save, "clicked");
    require(backend.writes == beforeRosterRevoke && attendeeInput->property("text") == "retained@example.test", "Revoked roster capability cannot silently omit authored guest changes");
    call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });
    call(details, "openAt", invitation, QVariant{}); settle([&] { return details->property("opened").toBool(); });
    call(named(root, "eventRsvp-accepted"), "clicked"); frozen = backend.lastWrite; backend.complete(true, resultFor(frozen, "reconciled"));
    require(details->property("feedback").toString().contains("observed") && details->property("feedback").toString().contains("unconfirmed"), "Observed RSVP is never presented as provider acceptance or notification delivery");
    call(details, "close"); settle([&] { return !details->property("visible").toBool(); });
    backend.calendarOperations = {QVariantMap{{"request_id", "observed"}, {"account", "alpha"}, {"account_name", "Alpha"}, {"target", QVariantMap{{"calendar", "primary"}}}, {"kind", "edit"}, {"state", "reconciled"}, {"revision", 21}}};
    emit backend.calendarOperationsChanged(); call(history, "open"); settle([&] { return history->property("opened").toBool(); });
    require(!named(root, "calendarOperationResolve-observed")->property("visible").toBool()
        && !named(root, "calendarOperationAbandon-observed")->property("visible").toBool(), "Terminal reconciled operation exposes no resend or abandon action");
    require(named(root, "calendarOperationEvidence-observed")->property("text").toString().contains("original response"), "Activity retains distinction between observed intent and provider response");
    call(history, "close"); settle([&] { return !history->property("visible").toBool(); });
    const QString captureDir = qEnvironmentVariable("FM_CALENDAR_CAPTURE_DIR");
    if (!captureDir.isEmpty()) {
        require(QDir().mkpath(captureDir), "Create calendar capture directory");
        auto window = qobject_cast<QQuickWindow *>(root); window->resize(390, 720);
        for (const auto theme : {"graphite", "paper", "terminal"}) for (const auto size : {"default", "large"}) {
            root->setProperty("testTheme", theme); root->setProperty("testTextSize", size); backend.events = {roster}; openEditor(roster);
            attendeeInput->setProperty("text", "long.guest.address.for.layout@example.test"); call(attendeeInput, "accepted");
            auto scroll = named(root, "eventFormScroll")->property("contentItem").value<QObject *>();
            require(scroll, "Native event form scroll");
            scroll->setProperty("contentY", std::max(0., scroll->property("contentHeight").toDouble() - scroll->property("height").toDouble()));
            QElapsedTimer paint; paint.start(); while (paint.elapsed() < 120) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
            auto notice = qobject_cast<QQuickItem *>(named(root, "eventNotificationExplanation"));
            const auto noticeRect = notice->mapRectToScene(notice->boundingRect());
            require(QRectF(0, 0, window->width(), window->height()).contains(noticeRect) && notice->property("truncated").toBool() == false, "Notification notice fits narrow default/large layout");
            const auto file = QDir(captureDir).filePath(QString("guests-%1-%2.png").arg(theme, size));
            require(!QFile::exists(file) && window->grabWindow().save(file), "Preserve calendar guest capture without overwrite");
            call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });
        }
        window->resize(1100, 850);
    }

    // Expanded master rows never identify an editable provider instance. The
    // real popup opens the shared editor only after its current exact read.
    auto recurring = original; recurring["id"] = "alpha:primary:series:original";
    recurring["event_id"] = "series"; recurring["etag"] = "master-v1";
    recurring["time_display"] = "09:00"; recurring["end_display"] = "10:00";
    recurring["start_hour"] = 9.; recurring["duration_hours"] = 1.;
    recurring["can_edit"] = false; recurring["can_delete"] = false; recurring["can_resolve_occurrence"] = true;
    const QVariantMap originalStart{{"kind", "zoned"}, {"instant", "2026-09-13T13:00:00Z"}, {"time_zone", "America/Toronto"}};
    recurring["occurrence_selector"] = QVariantMap{{"calendar_id", "primary"}, {"series_id", "series"}, {"original_start", originalStart}};
    backend.events = {recurring}; backend.account = "alpha"; emit backend.eventsChanged(); emit backend.accountChanged();
    const auto openOccurrence = [&] { call(details, "openAt", recurring, QVariant{}); settle([&] { return details->property("opened").toBool(); }); call(named(root, "eventEditButton"), "clicked"); };
    const auto resolved = [&](const QString &instanceEtag, const QString &masterEtag) {
        auto instance = original; instance["event_id"] = "opaque-instance"; instance["etag"] = instanceEtag;
        return QVariantMap{{"account", "alpha"}, {"calendar_id", "primary"}, {"event_id", "opaque-instance"}, {"etag", instanceEtag},
            {"occurrence", QVariantMap{{"series_id", "series"}, {"original_start", originalStart}, {"expected_master_etag", masterEtag}}}, {"event", instance}};
    };
    openOccurrence(); const auto closedRead = backend.lastOccurrenceCorrelation;
    require(backend.occurrenceReads == 1 && !editor->property("visible").toBool()
        && !backend.lastOccurrenceSelector.contains("event_id") && backend.lastOccurrenceSelector["account"] == "alpha", "master projection never opens or submits an instance editor");
    call(details, "close"); settle([&] { return !details->property("visible").toBool(); }); openOccurrence();
    const auto currentRead = backend.lastOccurrenceCorrelation;
    emit backend.calendarOccurrenceReadFinished(closedRead, true, resolved("instance-v1", "master-v1"), {});
    require(!editor->property("visible").toBool() && currentRead != closedRead, "same-occurrence close/reopen rejects the older read generation");
    auto sibling = recurring; sibling["id"] = "alpha:primary:series:other-original";
    auto siblingSelector = recurring["occurrence_selector"].toMap(), siblingStart = originalStart; siblingStart["instant"] = "2026-09-14T13:00:00Z";
    siblingSelector["original_start"] = siblingStart; sibling["occurrence_selector"] = siblingSelector;
    backend.events = {recurring, sibling}; emit backend.eventsChanged();
    call(details, "openAt", sibling, QVariant{}); call(named(root, "eventEditButton"), "clicked"); const auto siblingRead = backend.lastOccurrenceCorrelation;
    openOccurrence(); const auto returnedRead = backend.lastOccurrenceCorrelation;
    emit backend.calendarOccurrenceReadFinished(currentRead, true, resolved("instance-v1", "master-v1"), {});
    emit backend.calendarOccurrenceReadFinished(siblingRead, true, resolved("instance-v1", "master-v1"), {});
    require(!editor->property("visible").toBool(), "A-to-B-to-A selection rejects both prior read generations");
    emit backend.calendarOccurrenceReadFinished(returnedRead, true, resolved("instance-v1", "master-v1"), {});
    settle([&] { return editor->property("opened").toBool(); });
    title->setProperty("text", "Authored occurrence title"); call(editor, "save"); frozen = backend.lastWrite;
    require(frozen["event_id"] == "opaque-instance" && frozen["expected_etag"] == "instance-v1"
        && frozen["occurrence"].toMap()["expected_master_etag"] == "master-v1"
        && frozen["fields"].toMap() == QVariantMap{{"title", "Authored occurrence title"}}, "occurrence edit freezes resolved instance and master, omitting untouched fields");
    backend.complete(false, resultFor(frozen, "conflict", 30)); call(editor, "resolveOperation", "abandon"); backend.complete(false, resultFor(frozen, "abandoned", 31));
    recurring["etag"] = "master-v2"; backend.events = {recurring}; emit backend.eventsChanged();
    require(!save->property("enabled").toBool() && title->property("text") == "Authored occurrence title", "changed master requires explicit refresh and preserves authored text");
    call(named(root, "eventRefreshOccurrence"), "clicked"); const auto refreshRead = backend.lastOccurrenceCorrelation;
    title->setProperty("text", "Newer text while checking"); backend.account = "beta"; emit backend.accountChanged();
    emit backend.calendarOccurrenceReadFinished(refreshRead, true, resolved("instance-v2", "master-v2"), {});
    require(title->property("text") == "Newer text while checking" && plain(editor->property("editEvent")).toMap()["account"] == "alpha", "explicit same-occurrence refresh preserves newer fields and captured event account");
    call(editor, "deleteEvent"); frozen = backend.lastWrite;
    require(frozen["kind"] == "delete" && frozen["account"] == "alpha" && frozen["event_id"] == "opaque-instance"
        && frozen["expected_etag"] == "instance-v2" && frozen["occurrence"].toMap()["expected_master_etag"] == "master-v2"
        && !frozen.contains("fields"), "one-occurrence cancellation carries the refreshed exact immutable scope");
    backend.complete(true, resultFor(frozen, "reconciled", 32)); settle([&] { return !editor->property("visible").toBool(); });
    openOccurrence(); const auto removedEpochRead = backend.lastOccurrenceCorrelation;
    ++backend.occurrenceEpoch; emit backend.accountsChanged();
    emit backend.calendarOccurrenceReadFinished(removedEpochRead, true, resolved("instance-v3", "master-v2"), {});
    require(!editor->property("visible").toBool(), "remove and restore epoch cannot publish a held occurrence editor");
    call(details, "close"); settle([&] { return !details->property("visible").toBool(); }); openOccurrence();
    const auto withdrawnRead = backend.lastOccurrenceCorrelation; backend.events.clear(); emit backend.eventsChanged();
    emit backend.calendarOccurrenceReadFinished(withdrawnRead, true, resolved("instance-v3", "master-v2"), {});
    require(!editor->property("visible").toBool(), "source withdrawal rejects late resolved occurrence");
    settle([&] { return !details->property("visible").toBool(); }); backend.events = {recurring}; emit backend.eventsChanged(); openOccurrence();
    emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, false, {}, "This occurrence is cancelled.");
    require(!editor->property("visible").toBool() && details->property("feedback") == "This occurrence is cancelled.", "cancelled resolution remains a read-only popup error");
    call(details, "close"); settle([&] { return !details->property("visible").toBool(); }); backend.account = "alpha"; emit backend.accountChanged();

    // A retained child can change without changing its master-enriched ETag,
    // original key or instance hint. A completed old read must not reopen it.
    const auto unchangedChild = recurring;
    const QVariantList childChanges{QVariantMap{{"title", "Changed retained exception"}},
        QVariantMap{{"start", "2026-09-13T11:00:00-04:00"}, {"end", "2026-09-13T12:00:00-04:00"}},
        QVariantMap{{"attendees", QVariantList{QVariantMap{{"email", "changed-guest@example.test"}}}}}};
    for (const auto &value : childChanges) {
        openOccurrence(); const auto childRead = backend.lastOccurrenceCorrelation;
        const auto changes = value.toMap(); for (auto it = changes.begin(); it != changes.end(); ++it) recurring[it.key()] = it.value();
        backend.events = {recurring}; emit backend.eventsChanged();
        emit backend.calendarOccurrenceReadFinished(childRead, true, resolved("old-instance", "master-v2"), {});
        require(!editor->property("visible").toBool() && !details->property("feedback").toString().isEmpty(),
            "changed exception fields reject a held popup reply even with unchanged master identity");
        call(details, "close"); settle([&] { return !details->property("visible").toBool(); });
        recurring = unchangedChild; backend.events = {recurring}; emit backend.eventsChanged();
    }
    openOccurrence(); emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, true, resolved("instance-v3", "master-v2"), {});
    settle([&] { return editor->property("opened").toBool(); }); title->setProperty("text", "Authored text before child change");
    const auto readsBeforeChildChange = backend.occurrenceReads;
    recurring["attendees"] = childChanges.last().toMap()["attendees"]; backend.events = {recurring}; emit backend.eventsChanged();
    require(!save->property("enabled").toBool() && title->property("text") == "Authored text before child change"
        && backend.occurrenceReads == readsBeforeChildChange, "changed exception disables stale saving and retains fields without automatic resolution");
    call(named(root, "eventRefreshOccurrence"), "clicked"); const auto childRefresh = backend.lastOccurrenceCorrelation;
    title->setProperty("text", "Newer authored text during explicit child refresh");
    auto refreshedChild = resolved("instance-v4", "master-v2"); auto refreshedEvent = refreshedChild["event"].toMap();
    refreshedEvent["attendees"] = recurring["attendees"]; refreshedChild["event"] = refreshedEvent;
    emit backend.calendarOccurrenceReadFinished(childRefresh, true, refreshedChild, {});
    require(save->property("enabled").toBool() && title->property("text") == "Newer authored text during explicit child refresh",
        "explicit current-child resolution preserves newer authored fields and enables reviewed save");
    call(editor, "save"); frozen = backend.lastWrite;
    require(frozen["expected_etag"] == "instance-v4" && frozen["occurrence"].toMap()["expected_master_etag"] == "master-v2"
        && frozen["fields"].toMap() == QVariantMap{{"title", "Newer authored text during explicit child refresh"}},
        "refreshed child save uses its current instance version and only the retained authored change");
    backend.complete(true, resultFor(frozen, "accepted", 34)); settle([&] { return !editor->property("visible").toBool(); });
    recurring = unchangedChild; backend.events = {recurring}; emit backend.eventsChanged();

    // Mail's timeline uses its own retained event window, independently of the
    // Calendar workspace window. Exercise the same actual popup/editor path.
    details->setProperty("timelineSource", true); editor->setProperty("timelineSource", true);
    backend.events.clear(); backend.todayEvents = {recurring}; emit backend.eventsChanged(); openOccurrence();
    const auto timelineRead = backend.lastOccurrenceCorrelation;
    backend.events = {sibling}; emit backend.eventsChanged();
    emit backend.calendarOccurrenceReadFinished(timelineRead, true, resolved("instance-v3", "master-v2"), {});
    settle([&] { return editor->property("opened").toBool(); });
    title->setProperty("text", "Timeline occurrence edit"); call(editor, "save"); frozen = backend.lastWrite;
    require(frozen["account"] == "alpha" && frozen["event_id"] == "opaque-instance"
        && frozen["occurrence"].toMap()["original_start"].toMap() == originalStart,
        "timeline resolution and edit use the retained Mail window and exact occurrence");
    backend.complete(true, resultFor(frozen, "accepted", 33)); settle([&] { return !editor->property("visible").toBool(); });
    details->setProperty("timelineSource", false); editor->setProperty("timelineSource", false);
    backend.events = {recurring}; backend.todayEvents.clear(); emit backend.eventsChanged();

    backend.calendarOperations = {QVariantMap{{"request_id", "occurrence-observed"}, {"account", "alpha"}, {"account_name", "Alpha"},
        {"target", QVariantMap{{"calendar", "primary"}}}, {"kind", "delete"}, {"state", "reconciled"}, {"revision", 33},
        {"occurrence", resolved("instance-v2", "master-v2")["occurrence"]}}};
    emit backend.calendarOperationsChanged(); call(history, "open"); settle([&] { return history->property("opened").toBool(); });
    require(named(root, "calendarOperationOccurrence-occurrence-observed")->property("text").toString().contains("2026-09-13T13:00:00Z")
        && !named(root, "calendarOperationResolve-occurrence-observed")->property("visible").toBool(), "activity retains original occurrence identity without replaying reconciled cancellation");
    call(history, "close"); settle([&] { return !history->property("visible").toBool(); });
    if (!captureDir.isEmpty()) {
        auto window = qobject_cast<QQuickWindow *>(root); window->resize(390, 720);
        recurring["title"] = "A long recurring planning conversation across separate calendars"; backend.events = {recurring}; emit backend.eventsChanged();
        const auto retain = [&](const QString &name) {
            QElapsedTimer paint; paint.start(); while (paint.elapsed() < 120) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
            const auto file = QDir(captureDir).filePath(name + ".png"); require(!QFile::exists(file) && window->grabWindow().save(file), "preserve occurrence capture without overwrite");
        };
        const auto contained = [&](const char *name) {
            auto item = qobject_cast<QQuickItem *>(named(root, name)); require(item && item->isVisible(), "occurrence control visible");
            if (!QRectF(0, 0, window->width(), window->height()).contains(item->mapRectToScene(item->boundingRect())) || item->width() + .5 < item->implicitWidth())
                qWarning() << name << "geometry" << item->mapRectToScene(item->boundingRect()) << "implicit width" << item->implicitWidth();
            require(QRectF(0, 0, window->width(), window->height()).contains(item->mapRectToScene(item->boundingRect()))
                && item->width() + .5 >= item->implicitWidth(), "occurrence action remains contained with its full label");
        };
        for (const auto theme : {"graphite", "paper", "terminal"}) for (const auto size : {"default", "large"}) {
            root->setProperty("testTheme", theme); root->setProperty("testTextSize", size);
            call(details, "openAt", recurring, QVariant{}); settle([&] { return details->property("opened").toBool(); });
            retain(QString("occurrence-details-%1-%2").arg(theme, size)); contained("eventEditButton");
            call(named(root, "eventEditButton"), "clicked"); auto result = resolved("instance-v2", "master-v2");
            auto source = result["event"].toMap(); source["title"] = recurring["title"]; result["event"] = source;
            emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, true, result, {});
            settle([&] { return editor->property("opened").toBool(); });
            retain(QString("occurrence-editor-%1-%2").arg(theme, size));
            contained("eventDeleteButton"); contained("eventRefreshOccurrence"); contained("eventSaveButton");
            call(named(root, "eventDeleteButton"), "clicked"); settle([&] { return deleteDialog->property("opened").toBool(); });
            retain(QString("occurrence-cancel-%1-%2").arg(theme, size)); contained("eventConfirmDelete");
            call(deleteDialog, "reject"); call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });
        }
        window->resize(1100, 850);
    }

    // D038: the action scope is resolved before any write. A following split
    // has a separate explicit consequence confirmation and typed recovery.
    auto scopedSource = recurring;
    scopedSource["recurring_selector"] = scopedSource.take("occurrence_selector"); scopedSource["can_resolve_occurrence"] = false;
    scopedSource["recurring_scopes"] = QVariantMap{{"edit", QStringList{"occurrence", "series", "following"}},
        {"delete", QStringList{"occurrence", "series", "following"}}, {"rsvp", QStringList{"occurrence", "series"}}};
    backend.events = {scopedSource}; emit backend.eventsChanged();
    const auto scopedResult = [&](const QString &scope, const QString &action) {
        auto result = resolved("instance-v4", "master-v4"); result.remove("occurrence");
        QVariantMap context{{"scope", scope}, {"series_id", "series"}, {"expected_master_etag", "master-v4"}};
        if (scope != "series") context["original_start"] = originalStart;
        result["recurring"] = context;
        auto source = result["event"].toMap(); source["recurrence"] = QStringList{"RRULE:FREQ=DAILY;COUNT=6"};
        source["can_rsvp"] = action == "rsvp";
        if (scope == "series") { result["event_id"] = "series"; result["etag"] = "master-v4"; source["event_id"] = "series"; source["etag"] = "master-v4"; }
        result["event"] = source;
        result["following_notice"] = "Google resets later exceptions. Earlier exceptions remain. A replacement Google Meet uses a new meeting.";
        return result;
    };
    const auto openScope = [&](const QString &scope, const char *button) {
        call(details, "openAt", scopedSource, QVariant{}); settle([&] { return details->property("opened").toBool(); });
        details->setProperty("recurringScope", scope); call(named(root, button), "clicked");
        require(backend.lastOccurrenceSelector["scope"] == scope && backend.lastOccurrenceSelector["account"] == "alpha", "Scope read freezes action and event account");
    };
    for (const auto scope : {QString("occurrence"), QString("series")}) {
        openScope(scope, "eventEditButton");
        emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, true, scopedResult(scope, "edit"), {});
        settle([&] { return editor->property("opened").toBool(); });
        title->setProperty("text", "Scoped authored title"); call(editor, "save"); frozen = backend.lastWrite;
        require(frozen["recurring"].toMap()["scope"] == scope && !frozen.contains("occurrence")
            && frozen["event_id"] == (scope == "series" ? "series" : "opaque-instance")
            && frozen["fields"].toMap() == QVariantMap{{"title", "Scoped authored title"}}, "Scoped edits retain authoritative target and omit untouched recurrence/source fields");
        backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });
    }
    openScope("following", "eventEditButton"); const auto heldScope = backend.lastOccurrenceCorrelation;
    details->setProperty("recurringScope", "series");
    emit backend.calendarOccurrenceReadFinished(heldScope, true, scopedResult("following", "edit"), {});
    require(!editor->property("visible").toBool() && details->property("feedback").toString().contains("changed"), "Scope change invalidates a held lookup before any editor transition");
    call(details, "close"); settle([&] { return !details->property("visible").toBool(); });
    openScope("following", "eventEditButton");
    emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, true, scopedResult("following", "edit"), {});
    settle([&] { return editor->property("opened").toBool(); });
    require(!named(root, "eventRepeat")->property("visible").toBool() && named(root, "eventRetainedRecurrence")->property("visible").toBool(), "Following preserves the existing rule without offering unsupported recurrence changes");
    title->setProperty("text", "Following authored title"); const int beforeFollowing = backend.writes; call(editor, "save");
    auto followingDialog = named(root, "eventFollowingConfirmation"); settle([&] { return followingDialog->property("opened").toBool(); });
    require(backend.writes == beforeFollowing, "Following save does not trim before explicit consequence confirmation");
    call(followingDialog, "reject"); require(title->property("text") == "Following authored title", "Declining split confirmation retains authored text");
    call(editor, "save"); settle([&] { return followingDialog->property("opened").toBool(); }); call(followingDialog, "accept"); frozen = backend.lastWrite;
    require(frozen["recurring"].toMap()["reset_future_exceptions"] == true
        && frozen["recurring"].toMap()["original_start"].toMap() == originalStart, "Confirmed following write carries exact original and exception-reset choice");
    auto partial = resultFor(frozen, "partial", 41); partial["can_resume"] = false;
    partial["steps"] = QVariantList{QVariantMap{{"step", "trim"}, {"state", "acknowledged"}}, QVariantMap{{"step", "replacement"}, {"state", "not_attempted"}}};
    backend.complete(false, partial);
    require(editor->property("feedback").toString().contains("Old series trim: accepted by Google") && editor->property("feedback").toString().contains("Replacement: not attempted"), "Partial feedback identifies both provider steps without asserting notification delivery");
    call(editor, "save");
    require(backend.lastAction["action"] == "check" && backend.writes == beforeFollowing + 1, "Partial result without never-attempted proof cannot resume");
    partial["can_resume"] = true; backend.complete(false, partial);
    backend.active = false; emit backend.googleSyncChanged();
    require(!save->property("enabled").toBool() && editor->property("canAbandon").toBool(), "Partial Resume needs an active owner while local abandonment stays available");
    backend.active = true; emit backend.googleSyncChanged(); call(editor, "save");
    require(backend.lastAction["action"] == "resume" && backend.lastAction["request_id"] == frozen["request_id"] && backend.writes == beforeFollowing + 1, "Explicit partial Resume continues only recorded never-attempted work");
    auto replacementUnknown = resultFor(frozen, "unknown", 42); replacementUnknown["can_resume"] = false;
    replacementUnknown["steps"] = QVariantList{QVariantMap{{"step", "trim"}, {"state", "acknowledged"}}, QVariantMap{{"step", "replacement"}, {"state", "unknown"}}};
    backend.complete(false, replacementUnknown);
    require(editor->property("feedback").toString().contains("Old series trim: accepted by Google") && editor->property("feedback").toString().contains("Replacement: result unconfirmed"), "Unknown replacement retains known trim evidence");
    backend.capability = false; emit backend.accountsChanged();
    require(!save->property("enabled").toBool(), "Reconciliation cannot proceed without current detail-read grant");
    backend.readCapability = true; emit backend.accountsChanged();
    require(save->property("enabled").toBool(), "Unknown outcome remains inspectable with read-only Calendar grant");
    call(editor, "save"); require(backend.lastAction["action"] == "reconcile" && backend.writes == beforeFollowing + 1, "Unknown replacement only performs read reconciliation");
    backend.capability = true; backend.readCapability = false; emit backend.accountsChanged();
    backend.complete(true, resultFor(frozen, "acknowledged", 43)); settle([&] { return !editor->property("visible").toBool(); });
    openScope("series", "eventDeleteSeries");
    emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation,true,scopedResult("series","delete"),{});
    settle([&]{return directDeleteDialog->property("opened").toBool();});
    require(!editor->property("visible").toBool(),"Series deletion confirms directly in event details");
    call(named(root,"confirmDetailsDelete"),"clicked");frozen=backend.lastWrite;
    require(frozen["kind"]=="delete"&&frozen["event_id"]=="series"&&frozen["expected_etag"]=="master-v4"&&frozen["recurring"].toMap()["scope"]=="series","Delete series uses freshly resolved master identity");
    backend.complete(true,resultFor(frozen,"acknowledged"));settle([&]{return !details->property("visible").toBool();});
    openScope("following", "eventScopedCancel");
    require(backend.lastOccurrenceSelector["action"] == "delete", "Cancellation resolves its own permitted action");
    emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, true, scopedResult("following", "delete"), {});
    settle([&] { return deleteDialog->property("opened").toBool(); }); call(deleteDialog, "accept"); frozen = backend.lastWrite;
    require(frozen["kind"] == "delete" && frozen["recurring"].toMap()["reset_future_exceptions"] == true, "Following cancellation requires exact reset confirmation");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });
    openScope("series", "eventRsvp-accepted"); const int beforeRsvpScope = backend.writes;
    require(backend.lastOccurrenceSelector["action"] == "rsvp", "Recurring RSVP first resolves participant eligibility");
    emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, true, scopedResult("series", "rsvp"), {});
    frozen = backend.lastWrite;
    require(backend.writes == beforeRsvpScope + 1 && frozen["kind"] == "rsvp" && frozen["event_id"] == "series"
        && frozen["recurring"].toMap()["scope"] == "series", "Series RSVP uses the verified master and captured scope");
    require(backend.signaledResponse == frozen, "Pending RSVP property signal contains the complete actual scoped request before dispatch");
    backend.complete(false, resultFor(frozen, "unknown", 44));
    backend.capability = false; backend.readCapability = true; emit backend.accountsChanged();
    require(named(root, "eventRsvpResolve")->property("enabled").toBool(), "Scoped RSVP outcome remains inspectable after write grant loss");
    call(named(root, "eventRsvpResolve"), "clicked"); require(backend.lastAction["action"] == "reconcile", "Actual RSVP recovery control selects read reconciliation");
    backend.capability = true; backend.readCapability = false; emit backend.accountsChanged();
    backend.complete(true, resultFor(frozen, "acknowledged")); call(details, "close"); settle([&] { return !details->property("visible").toBool(); });
    openEditor({}); title->setProperty("text", "Repeating new event");
    auto repeat = named(root, "eventRepeat"); repeat->setProperty("currentIndex", 1); require(QMetaObject::invokeMethod(repeat, "activated", Q_ARG(int, 1)), "Select native Repeat control");
    named(root, "eventRepeatInterval")->setProperty("text", "2"); named(root, "eventRepeatCount")->setProperty("text", "5"); call(editor, "save"); frozen = backend.lastWrite;
    require(frozen["fields"].toMap()["recurrence"].toStringList() == QStringList{"RRULE:FREQ=DAILY;INTERVAL=2;COUNT=5"}, "Repeat control emits the shared common-rule array");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });

    openEditor({}); title->setProperty("text", "Custom weekday series");
    named(root, "eventReminder")->setProperty("currentIndex", 6);
    named(root, "eventAvailability")->setProperty("currentIndex", 1);
    call(named(root, "eventAddMeet"), "clicked");
    repeat->setProperty("currentIndex", 5); require(QMetaObject::invokeMethod(repeat, "activated", Q_ARG(int, 5)), "Choose custom recurrence");
    call(named(root, "eventRepeatDay_MO"), "clicked"); call(named(root, "eventRepeatDay_WE"), "clicked"); call(named(root, "eventRepeatDay_SU"), "clicked");
    named(root, "eventRepeatInterval")->setProperty("text", "2");
    named(root, "eventRepeatEnd")->setProperty("currentIndex", 1);
    named(root, "eventRepeatUntil")->setProperty("text", "2026-10-31");
    call(editor, "save"); frozen = backend.lastWrite;
    require(frozen["fields"].toMap()["recurrence"].toStringList() == QStringList{"RRULE:FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;UNTIL=20261101T035959Z"}, "Custom weekdays and inclusive local end date become a bounded UTC RRULE");
    require(frozen["fields"].toMap()["reminder_minutes"] == 30 && frozen["fields"].toMap()["availability"] == "free" && frozen["fields"].toMap()["conference"] == "google_meet", "Visible reminder, availability and Meet controls enter the same frozen request");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });

    openEditor({}); title->setProperty("text", "Three days away");
    named(root, "eventAllDay")->setProperty("checked", true);
    named(root, "eventStartDate")->setProperty("text", "2026-09-13"); named(root, "eventEndDate")->setProperty("text", "2026-09-15");
    repeat->setProperty("currentIndex", 2); require(QMetaObject::invokeMethod(repeat, "activated", Q_ARG(int, 2)), "Choose weekdays preset");
    named(root, "eventRepeatEnd")->setProperty("currentIndex", 1); named(root, "eventRepeatUntil")->setProperty("text", "2026-09-30");
    call(editor, "save"); frozen = backend.lastWrite;
    require(frozen["fields"].toMap()["start"] == "2026-09-13" && frozen["fields"].toMap()["end"] == "2026-09-16", "All-day Ends includes the selected day and sends Google's exclusive endpoint");
    require(frozen["fields"].toMap()["recurrence"].toStringList() == QStringList{"RRULE:FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;UNTIL=20260930"}, "All-day weekday recurrence retains DATE end semantics");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });
    auto multiDay = event(); multiDay["all_day"] = true; multiDay["start"] = "2026-09-13"; multiDay["end"] = "2026-09-16";
    openEditor(multiDay); require(named(root, "eventEndDate")->property("text") == "2026-09-15", "Existing multi-day end date opens inclusively");
    title->setProperty("text", "Only the title changes"); call(editor, "save"); frozen = backend.lastWrite;
    require(!frozen["fields"].toMap().contains("end"), "Title-only edit preserves exact existing all-day endpoints");
    backend.complete(true, resultFor(frozen, "acknowledged")); settle([&] { return !editor->property("visible").toBool(); });
    openEditor({}); title->setProperty("text", "Invalid date stays editable");
    named(root, "eventStartDate")->setProperty("text", "2026-02-29"); const auto beforeInvalidDate = backend.writes; call(editor, "save");
    require(backend.writes == beforeInvalidDate && named(root, "eventStartDate")->property("text") == "2026-02-29", "Invalid leap date is retained without submitting");
    call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });

    if (!captureDir.isEmpty()) {
        auto window = qobject_cast<QQuickWindow *>(root); window->resize(390, 720);
        const auto retainNew = [&](const QString &name) {
            QElapsedTimer paint; paint.start(); while (paint.elapsed() < 120) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
            const auto file = QDir(captureDir).filePath(name + ".png"); require(!QFile::exists(file) && window->grabWindow().save(file), "Preserve new reference controls without overwrite");
        };
        for (const auto theme : {"graphite", "paper", "terminal"}) for (const auto size : {"default", "large"}) {
            root->setProperty("testTheme", theme); root->setProperty("testTextSize", size);
            call(details, "openAt", scopedSource, QVariant{}); settle([&] { return details->property("opened").toBool(); });
            details->setProperty("recurringScope", "following"); retainNew(QString("recurring-scope-%1-%2").arg(theme, size));
            call(named(root, "eventEditButton"), "clicked"); emit backend.calendarOccurrenceReadFinished(backend.lastOccurrenceCorrelation, true, scopedResult("following", "edit"), {});
            settle([&] { return editor->property("opened").toBool(); }); title->setProperty("text", "Following planning session");
            call(editor, "save"); settle([&] { return followingDialog->property("opened").toBool(); });
            retainNew(QString("following-confirmation-%1-%2").arg(theme, size));
            auto confirm = qobject_cast<QQuickItem *>(named(root, "eventConfirmFollowing"));
            require(confirm && QRectF(0, 0, window->width(), window->height()).contains(confirm->mapRectToScene(confirm->boundingRect())), "Following confirmation fits narrow large-font window");
            call(followingDialog, "reject"); call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });
            openEditor({}); title->setProperty("text", "Weekly planning"); repeat->setProperty("currentIndex", 2);
            require(QMetaObject::invokeMethod(repeat, "activated", Q_ARG(int, 2)), "Show recurring-create controls");
            auto scroll = named(root, "eventFormScroll")->property("contentItem").value<QObject *>();
            scroll->setProperty("contentY", std::max(0., scroll->property("contentHeight").toDouble() - scroll->property("height").toDouble()));
            retainNew(QString("repeat-create-%1-%2").arg(theme, size));
            call(editor, "close"); settle([&] { return !editor->property("visible").toBool(); });
        }
        window->resize(1100, 850);
    }
    backend.syntheticProfile = true; emit backend.modeChanged(); openEditor({});
    title->setProperty("text", "Synthetic event"); named(root, "eventAttendees")->setProperty("text", "guest@example.test"); call(editor, "save"); frozen = backend.lastWrite;
    require(frozen["fields"].toMap().contains("conference_url") && frozen["fields"].toMap()["attendees"].toStringList().contains("guest@example.test"), "Synthetic form retains Phase 1 attendees and complete fields");
    auto local = resultFor(frozen, "fake_acknowledged"); backend.complete(true, local); settle([&] { return !editor->property("visible").toBool(); });
    backend.syntheticProfile = false; emit backend.modeChanged(); backend.account = "alpha"; emit backend.accountChanged();
    auto visualA = event(); visualA["start_display"] = "Sun 13 Sep 09:00"; visualA["time_display"] = "09:00"; visualA["end_display"] = "10:00"; visualA["day_index"] = 0; visualA["start_hour"] = 9.; visualA["duration_hours"] = 1.;
    auto visualB = visualA; visualB["id"] = "beta:event"; visualB["account"] = "beta"; visualB["event_id"] = "beta-event";
    backend.events = {visualA, visualB}; emit backend.eventsChanged();
    QQmlApplicationEngine visualEngine; FmIconItem::registerType(); visualEngine.rootContext()->setContextProperty("backend", &backend);
    QObject::connect(&visualEngine, &QQmlEngine::warnings, [&](const QList<QQmlError> &errors) { qmlError = true; for (const auto &error : errors) qWarning() << error.toString(); });
    visualEngine.loadData(R"QML(import QtQuick
import QtQuick.Controls
ApplicationWindow { width: 1100; height: 850; visible: true
    property string testTheme: "graphite"
    color: Theme.canvas
    Binding { target: Theme; property: "themeId"; value: testTheme }
    CalendarView { anchors.fill: parent }
    StartupOverlay { objectName: "readyLoader"; anchors.fill: parent }
    StartupOverlay { objectName: "failedLoader"; anchors.fill: parent; startupFailed: true }
    StartupOverlay { objectName: "unavailableLoader"; anchors.fill: parent; startupFailed: true; engineUnavailable: true
        failureMessage: "This profile was made by an older fuckMail and cannot be opened."; profilePath: "/synthetic/profile" }
})QML", QUrl::fromLocalFile(copy.filePath("VisualTest.qml")));
    require(!visualEngine.rootObjects().isEmpty(), "Actual calendar and startup component load");
    auto visualRoot = visualEngine.rootObjects().first(); auto visualWindow = qobject_cast<QQuickWindow *>(visualRoot);
    auto loader = named(visualRoot, "readyLoader"); require(loader->property("visible").toBool(), "Startup stays covered before hydration");
    require(!named(visualRoot, "failedLoader")->property("visible").toBool(), "Startup failure reveals existing recovery immediately");
    auto unavailable = named(visualRoot, "unavailableLoader");
    require(unavailable->property("visible").toBool() && named(unavailable, "startupFailure")->property("visible").toBool()
        && named(unavailable, "startupRetry")->property("visible").toBool(), "An engine that never started keeps covering the empty app and explains why");
    const auto retainVisual = [&](const QString &name) {
        QElapsedTimer paint; paint.start(); while (paint.elapsed() < 120) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
        const auto file = QDir(captureDir).filePath(name + ".png"); require(!QFile::exists(file) && visualWindow->grabWindow().save(file), "Preserve startup/grid capture without overwrite");
    };
    if (!captureDir.isEmpty()) {
        QElapsedTimer animation; animation.start(); while (animation.elapsed() < 1500) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
        for (const auto theme : {"graphite", "paper", "terminal"}) { visualRoot->setProperty("testTheme", theme); retainVisual(QString("startup-unavailable-%1").arg(theme)); }
    }
    unavailable->setProperty("visible", false);
    if (!captureDir.isEmpty())
        for (const auto theme : {"graphite", "paper", "terminal"}) { visualRoot->setProperty("testTheme", theme); retainVisual(QString("startup-%1").arg(theme)); }
    // Readiness no longer skips the first-frame splash hold. Let the actual
    // animation progress before applying the existing transition watchdog.
    QElapsedTimer splashHold; splashHold.start();
    while (splashHold.elapsed() < 1600) QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    require(loader->property("visible").toBool(), "Slow hydration retains splash after animation");
    loader->setProperty("startupReady", true); settle([&] { return !loader->property("visible").toBool(); });
    loader->setProperty("startupReady", false); require(!loader->property("visible").toBool(), "Later queries cannot replay startup animation");
    auto calendar = named(visualRoot, "calendarWorkspace");
    require(plain(calendar->property("visibleEvents")).toList().size() == 2, "Calendar defaults to combined permitted projection");
    auto scopeNotice = named(visualRoot, "calendarHiddenScopeNotice");
    require(!scopeNotice->property("visible").toBool(), "All-calendar view has no hidden-scope notice");
    backend.preferences["selected_calendars"] = QVariantList{QVariantMap{{"account", "beta"}, {"calendar_id", "primary"}}};
    emit backend.eventsChanged();
    settle([&] { return scopeNotice->property("visible").toBool(); });
    require(calendar->property("hiddenCalendarCount").toInt() == 2, "Restricted legacy selection discloses every hidden calendar in the current account scope");
    require(!calendar->property("combinedAccounts").toBool() && !named(visualRoot, "calendarCombinedToggle")->property("checked").toBool(), "A saved hidden selection cannot claim All calendars is on");
    call(named(visualRoot, "calendarShowAllFromNotice"), "clicked");
    settle([&] { return !scopeNotice->property("visible").toBool(); });
    require(backend.preferences.value("selected_calendars").isNull() && backend.preferences.value("calendar_account_visibility") == "all" && calendar->property("combinedAccounts").toBool(), "Show all restores the combined calendar scope");
    backend.preferences["calendar_account_visibility"] = "current"; emit backend.eventsChanged();
    settle([&] { return scopeNotice->property("visible").toBool(); });
    require(calendar->property("selectedCalendarCount").toInt() == 1, "Saved current-account scope discloses the other calendar");
    call(named(visualRoot, "calendarShowAllFromNotice"), "clicked");
    settle([&] { return !scopeNotice->property("visible").toBool(); });
    require(backend.preferences.value("calendar_account_visibility") == "all", "Show all clears saved account restriction");
    const auto savedSelection = backend.preferences.value("selected_calendars");
    backend.setPreferences({{"calendar_account_visibility","current"}});
    require(plain(calendar->property("visibleEvents")).toList().size() == 1 && backend.preferences.value("selected_calendars") == savedSelection, "Account-only display preserves saved calendar selection");
    backend.account = "beta"; emit backend.accountChanged();
    require(plain(calendar->property("visibleEvents")).toList().first().toMap()["account"] == "beta", "Account-only display follows explicit selected account");
    backend.account = "alpha"; emit backend.accountChanged(); backend.setPreferences({{"calendar_account_visibility","all"}});
    auto slot = qobject_cast<QQuickItem *>(named(visualRoot, "calendarSlot-0")); auto gridEditor = named(visualRoot, "eventEditor");
    const auto nativeMouse = [&](QEvent::Type type, const QPointF &point, Qt::MouseButton button, Qt::MouseButtons buttons) {
        QMouseEvent mouse(type, point, visualWindow->mapToGlobal(point.toPoint()), button, buttons, Qt::NoModifier); QCoreApplication::sendEvent(visualWindow, &mouse);
    };
    settle([&] { return slot->width() > 0 && slot->height() > 0; });
    const auto hh = calendar->property("hourHeight").toDouble();
    // Hovering an empty half hour shows the slot a click would create; an
    // event, or a slot running into one, shows none.
    auto ghost = named(visualRoot, "calendarSlotGhost");
    nativeMouse(QEvent::MouseMove, slot->mapToScene({slot->width() / 2, hh * 14.6}), Qt::NoButton, Qt::NoButton);
    settle([&] { return ghost->property("visible").toBool(); });
    require(qAbs(ghost->property("y").toDouble() - hh * 14.5) < .5 && ghost->property("height").toDouble() >= 20, "The ghost covers the hovered half hour, at least 20px tall");
    nativeMouse(QEvent::MouseMove, slot->mapToScene({slot->width() / 4, hh * 9.4}), Qt::NoButton, Qt::NoButton);
    settle([&] { return !ghost->property("visible").toBool(); });
    nativeMouse(QEvent::MouseMove, slot->mapToScene({slot->width() / 2, hh * 23.9}), Qt::NoButton, Qt::NoButton);
    settle([&] { return ghost->property("visible").toBool(); });
    require(ghost->property("y").toDouble() + ghost->property("height").toDouble() <= hh * 24 + .5, "The ghost never runs past 24:00");
    const auto startPoint = slot->mapToScene({slot->width() / 2, hh * 12 + 2});
    const auto endPoint = slot->mapToScene({slot->width() / 2, hh * 13.5});
    nativeMouse(QEvent::MouseButtonPress, startPoint, Qt::LeftButton, Qt::LeftButton);
    nativeMouse(QEvent::MouseMove, endPoint, Qt::NoButton, Qt::LeftButton);
    require(named(visualRoot, "calendarRangePreview")->property("visible").toBool(), "Native empty-grid drag paints range before opening editor");
    if (!captureDir.isEmpty()) for (const auto theme : {"graphite", "paper", "terminal"}) { visualRoot->setProperty("testTheme", theme); retainVisual(QString("calendar-range-%1").arg(theme)); }
    nativeMouse(QEvent::MouseButtonRelease, endPoint, Qt::LeftButton, Qt::NoButton);
    settle([&] { return gridEditor->property("opened").toBool(); });
    require(named(visualRoot, "eventStartTime")->property("text") == "12:00" && named(visualRoot, "eventEndTime")->property("text") == "13:30", "Actual range drag transfers snapped start and end to native editor");
    call(gridEditor, "close"); settle([&] { return !gridEditor->property("visible").toBool(); });
    call(calendar, "beginRange", 0, hh * 12); backend.account = "beta"; emit backend.accountChanged(); call(calendar, "finishRange");
    require(!gridEditor->property("opened").toBool(), "Account change cancels range intent before editor creation");
    require(QMetaObject::invokeMethod(gridEditor, "setSlotRange", Q_ARG(QVariant, "2026-09-13"), Q_ARG(QVariant, 23.5), Q_ARG(QVariant, 24.)), "Set explicit midnight range");
    require(named(visualRoot, "eventEndDate")->property("text") == "2026-09-14" && named(visualRoot, "eventEndTime")->property("text") == "00:00", "Midnight drag creates a real next-day endpoint");
    backend.account = "alpha"; emit backend.accountChanged(); backend.calendarView = "week"; emit backend.eventsChanged();
    named(visualRoot, "eventEndDate")->setProperty("text", "2026-09-15");
    named(visualRoot, "eventEndTime")->setProperty("text", "01:30");
    call(gridEditor, "open");
    settle([&] { return plain(calendar->property("previewRows")).toList().size() == 3; });
    const auto previews = plain(calendar->property("previewRows")).toList();
    require(previews[0].toMap()["duration_hours"].toDouble() == .5 && previews[1].toMap()["duration_hours"].toDouble() == 24 && previews[2].toMap()["duration_hours"].toDouble() == 1.5, "Native editor preview uses all covered civil days");
    require(named(visualRoot, "calendarEditorPreview-2"), "Last multi-day preview segment exists");
    if (!captureDir.isEmpty()) retainVisual("calendar-multiday-preview");
    call(gridEditor, "close");
    backend.calendarBandRows = 2; emit backend.eventsChanged();
    require(calendar->property("bandHeight").toDouble() == 92, "Two all-day rows make a 92px band");
    for (const auto view : {"day", "week", "month"}) {
        backend.setCalendarView(view);
        QCoreApplication::processEvents();
        auto band = qobject_cast<QQuickItem *>(named(visualRoot, QString(view) == "month" ? "calendarMonthBand" : "calendarBand"));
        settle([&] { return qAbs(band->height() - 92) < .5; });
        if (QString(view) == "day") {
            // Day view shows the day's mail above its agenda, not in the band.
            auto received = named(visualRoot, "calendarDayMailReceived"), sent = named(visualRoot, "calendarDayMailSent");
            require(qobject_cast<QQuickItem *>(named(visualRoot, "calendarDayMail"))->height() == 92, "The Day mail row is as tall as the band");
            require(received->property("text").toString() == "7" && sent->property("text").toString() == "2", "Day sidebar counts use the selected mail account");
            backend.setPreferences({{"calendar_account_visibility","current"}});
            require(!backend.mailCombined && received->property("text").toString() == "7", "Event scope does not change mail count account");
            backend.setPreferences({{"calendar_account_visibility","all"}});
        } else {
            const auto labelName = (QString(view) == "month" ? "calendarMonthMailCount-" : "calendarMailCount-") + backend.calendarFrom;
            const auto label = named(visualRoot, labelName.toUtf8().constData());
            require(label && label->property("ready").toBool(), "Daily mail pill exists in week and month");
            require(label->property("day").toMap().value("received") == 7 && label->property("day").toMap().value("sent") == 2, "Counts use the selected mail account");
            backend.setPreferences({{"calendar_account_visibility","current"}});
            require(!backend.mailCombined && label->property("day").toMap().value("received") == 7, "Event scope does not change mail count account");
            backend.setPreferences({{"calendar_account_visibility","all"}});
        }
        if (!captureDir.isEmpty()) {
            for (const auto theme : {"graphite", "paper", "harbour", "terminal"}) {
                visualRoot->setProperty("testTheme", theme);
                retainVisual(QString("calendar-mail-%1-%2").arg(view, theme));
            }
        }
    }
    backend.setCalendarView("month"); QCoreApplication::processEvents();
    auto monthEvent=qobject_cast<QQuickItem *>(named(visualRoot,"calendarMonthEvent-event"));
    require(monthEvent->isVisible()&&monthEvent->width()>0,"Month event is a separate target");
    const auto eventPoint=monthEvent->mapToScene({monthEvent->width()/2,monthEvent->height()/2});
    nativeMouse(QEvent::MouseButtonPress,eventPoint,Qt::LeftButton,Qt::LeftButton);
    nativeMouse(QEvent::MouseButtonRelease,eventPoint,Qt::LeftButton,Qt::NoButton);
    settle([&]{return named(visualRoot,"eventDetails")->property("visible").toBool();});
    require(backend.calendarView=="month","Month event click does not open Day view");
    call(named(visualRoot,"eventDetails"),"close");
    auto monthCell=qobject_cast<QQuickItem *>(named(visualRoot,"calendarMonthCell-2026-09-13"));
    const auto emptyPoint=monthCell->mapToScene({monthCell->width()-5,monthCell->height()-5});
    nativeMouse(QEvent::MouseButtonPress,emptyPoint,Qt::LeftButton,Qt::LeftButton);
    nativeMouse(QEvent::MouseButtonRelease,emptyPoint,Qt::LeftButton,Qt::NoButton);
    require(backend.calendarView=="day"&&backend.calendarDate=="2026-09-13","Month empty cell opens the selected Day view");
    // Month's week axis: ISO numbers in one muted colour, each opening its week.
    backend.setCalendarView("month"); QCoreApplication::processEvents();
    auto thisWeek=qobject_cast<QQuickItem *>(named(visualRoot,"calendarWeekNumber-0")), nextWeek=qobject_cast<QQuickItem *>(named(visualRoot,"calendarWeekNumber-1"));
    settle([&]{return nextWeek->isVisible()&&nextWeek->height()>0;});
    require(thisWeek->property("number").toInt()==38&&thisWeek->property("current").toBool()&&nextWeek->property("number").toInt()==39&&!nextWeek->property("current").toBool(),"Week numbers follow ISO weeks and mark the current one");
    const auto weekPoint=nextWeek->mapToScene({nextWeek->width()/2,12});
    nativeMouse(QEvent::MouseButtonPress,weekPoint,Qt::LeftButton,Qt::LeftButton);
    nativeMouse(QEvent::MouseButtonRelease,weekPoint,Qt::LeftButton,Qt::NoButton);
    require(backend.calendarView=="week"&&backend.calendarDate=="2026-09-20","A week number opens that week in Week view");
    // Mail's Next row: Day view on the event's day, its card open beside its block as if clicked.
    call(calendar,"showEventInDay",visualA);
    auto dayDetails=named(visualRoot,"eventDetails");
    settle([&]{return dayDetails->property("opened").toBool();});
    auto dayBlock=qobject_cast<QQuickItem *>(named(visualRoot,"calendarEvent-event"));
    require(backend.calendarView=="day"&&backend.calendarDate=="2026-09-13"&&plain(dayDetails->property("eventData")).toMap()["id"]==visualA["id"]
        &&dayBlock&&dayBlock->isVisible()&&qAbs(dayDetails->property("originY").toDouble()-dayBlock->mapToScene({0,dayBlock->height()}).y())<1,"showEventInDay opens the event's Day view card at its block");
    call(dayDetails,"close");settle([&]{return !dayDetails->property("visible").toBool();});
    // An invitation email's event is Core's row, with no layout date: its own day, not today.
    auto undated=visualA; undated.remove("date"); undated["start"]="2026-09-17T02:00:00Z"; undated["end"]="2026-09-17T03:00:00Z";
    backend.setCalendarView("week"); call(calendar,"showEventInDay",undated);
    require(backend.calendarView=="day"&&backend.calendarDate=="2026-09-16","showEventInDay without a layout date opens the event's display-zone day");
    backend.setCalendarDate("2026-09-13");
    backend.setCalendarView("week");
    backend.calendarBandRows=0; emit backend.eventsChanged();
    calendar->setProperty("visible", false); require(!backend.mailActive, "Hidden Calendar stops count queries");
    visualWindow->close();
    require(!qmlError, "Actual controls run without QML binding errors");
    qInfo() << "Calendar and label controls passed: partial live patches, frozen scope, recovery, sync/capability gates, delete placement, RSVP projection, cached-only labels, offline local binding, explicit remap and synthetic compatibility.";
}

#include "CalendarControlsTest.moc"
