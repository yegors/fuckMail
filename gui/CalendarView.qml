import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: calendar
    objectName: "calendarWorkspace"
    signal openMailRequested()
    property int dayCount: backend.calendarView === "day" ? 1 : 7
    readonly property bool combinedAccounts: backend.preferences.calendar_account_visibility !== "current"
        && (backend.preferences.selected_calendars === null || backend.preferences.selected_calendars === undefined)
    readonly property bool accountScopeRestricted: !combinedAccounts
    readonly property int permittedCalendarCount: backend.calendarSources.filter(function(source) { return source.permission !== "none" }).length
    readonly property int selectedCalendarCount: {
        const selected = backend.preferences.selected_calendars
        return backend.calendarSources.filter(function(source) {
            if (source.permission === "none" || (accountScopeRestricted && source.account !== backend.account)) return false
            return selected === null || selected === undefined || selected.some(function(row) {
                return row.account === source.account && row.calendar_id === source.calendar_id
            })
        }).length
    }
    readonly property int hiddenCalendarCount: Math.max(0, permittedCalendarCount - selectedCalendarCount)
    readonly property var mailActivity: backend.calendarMailActivity || ({state: "unavailable"})
    // Ready counts by date, shared by every day badge.
    readonly property var mailDays: {
        if (mailActivity.state !== "ready") return ({})
        if (mailActivity.by_date) return mailActivity.by_date
        const days = {}
        for (const row of mailActivity.days || []) days[row.date] = row
        return days
    }
    function updateMailActivityContext() {
        if (backend.setCalendarMailActivityContext !== undefined)
            backend.setCalendarMailActivityContext(visible && backend.calendarView !== "agenda", false)
    }
    Component.onCompleted: updateMailActivityContext()
    readonly property var previewRows: { backend.calendarDate; backend.calendarView; backend.preferences; return editor.opened ? backend.calendarPreview(editor.preview) : [] }
    function editingEvent(event) { return editor.opened && editor.preview.eventId === event.event_id && editor.preview.account === event.account && editor.preview.calendar_id === event.calendar_id }
    function prepareSearch() {
        if (editor.opened || operationHistory.opened) return false
        cancelRange(); details.close()
        return true
    }
    property var visibleEvents: combinedAccounts ? backend.events : backend.events.filter(function(e) { return e.account === backend.account })
    readonly property real workingStart: backend.preferences.timeline_start_hour === undefined ? 7 : backend.preferences.timeline_start_hour
    readonly property real workingEnd: backend.preferences.timeline_end_hour === undefined ? 21 : backend.preferences.timeline_end_hour
    property var rangeDrag: ({})
    property var agendaEvents: {
        let seen = {}
        return visibleEvents.filter(function(e) { if (seen[e.id]) return false; seen[e.id] = true; return true }).sort(function(a, b) {
            if (a.date !== b.date) return a.date < b.date ? -1 : 1
            if (a.all_day !== b.all_day) return a.all_day ? -1 : 1
            if (a.start_hour !== b.start_hour) return a.start_hour - b.start_hour
            return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
        })
    }
    // A quick load shows no progress text, so the grid does not jump twice.
    readonly property bool calendarLoading: !backend.calendarReady && (backend.calendarCoverage === "Loading calendars…" || backend.calendarCoverage === "Refreshing cached calendar…")
    property bool slowLoad: false
    onCalendarLoadingChanged: if (!calendarLoading) slowLoad = false
    Timer { interval: 400; running: calendar.calendarLoading; onTriggered: calendar.slowLoad = true }
    property real hourHeight: Math.max(24, Math.floor((gridScroll.height - 10) / 24))
    property real retainedTopHour: 0
    property real laidOutHourHeight: 24
    property real laidOutGridHeight: -1
    property bool restoringGridPosition: false
    property int monthCount: {
        let selected = new Date(backend.calendarDate + "T12:00:00")
        let end = new Date(selected.getFullYear(), selected.getMonth() + 1, 0, 12)
        return Math.ceil((Math.round((end - dateAt(0)) / 86400000) + 1) / 7) * 7
    }
    readonly property bool popupOpen: editor.opened || details.opened || operationHistory.opened || datePicker.opened || moreMenu.opened || allDayOverflow.opened
    property bool shortcutsEnabled: visible && (!backend.reminders || backend.reminders.length === 0) && !editingField() && !popupOpen
    function editingField() {
        let item = Window.window ? Window.window.activeFocusItem : null
        while (item) { if (item.blocksWorkspaceShortcuts || (item.cursorPosition !== undefined && item.text !== undefined)) return true; item = item.parent }
        return false
    }
    // One Date per grid day, rebuilt only when the range moves. Callers must not mutate them.
    readonly property var rangeDates: {
        const start = new Date(backend.calendarFrom + "T12:00:00"), days = []
        const two = function(value) { return value < 10 ? "0" + value : "" + value }
        for (let index = 0; index < 42; ++index) {
            const date = new Date(start); date.setDate(start.getDate() + index)
            days.push({date: date, iso: date.getFullYear() + "-" + two(date.getMonth() + 1) + "-" + two(date.getDate())})
        }
        return days
    }
    function dateAt(index) {
        if (index >= 0 && index < rangeDates.length) return rangeDates[index].date
        let date = new Date(backend.calendarFrom + "T12:00:00"); date.setDate(date.getDate() + index); return date
    }
    function isoAt(index) { return index >= 0 && index < rangeDates.length ? rangeDates[index].iso : Qt.formatDate(dateAt(index), "yyyy-MM-dd") }
    // Hidden views get empty models, so they create and re-evaluate nothing.
    readonly property bool timeGrid: backend.calendarView === "day" || backend.calendarView === "week"
    readonly property bool monthView: backend.calendarView === "month"
    // Month cells stay built and keep their last values while another view shows,
    // so neither leaving the month nor navigating elsewhere touches them.
    property var monthDates: []
    property var monthEvents: []
    property int monthNumber: -1
    property int monthCells: 42
    Binding on monthDates { value: calendar.rangeDates; when: calendar.monthView; restoreMode: Binding.RestoreNone }
    Binding on monthEvents { value: calendar.eventsByDay; when: calendar.monthView; restoreMode: Binding.RestoreNone }
    Binding on monthNumber { value: new Date(backend.calendarDate + "T12:00:00").getMonth(); when: calendar.monthView; restoreMode: Binding.RestoreNone }
    Binding on monthCells { value: calendar.monthCount; when: calendar.monthView; restoreMode: Binding.RestoreNone }
    readonly property var emptyDay: ({date: new Date(NaN), iso: ""})
    function weekNumber() { let date = dateAt(3); let utc = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())); utc.setUTCDate(utc.getUTCDate() + 4 - (utc.getUTCDay() || 7)); return Math.ceil((((utc - new Date(Date.UTC(utc.getUTCFullYear(), 0, 1))) / 86400000) + 1) / 7) }
    function rangeTitle() { return Qt.formatDate(dateAt(0), dateAt(0).getMonth() === dateAt(6).getMonth() ? "d" : "d MMM") + " – " + Qt.formatDate(dateAt(6), "d MMMM") }
    // Events bucketed by grid day once per change, in presentation order.
    readonly property var eventsByDay: {
        const days = []
        for (let index = 0; index < 42; ++index) days.push([])
        for (const e of visibleEvents) {
            if (!e.all_day) { if (e.day_index >= 0 && e.day_index < 42) days[e.day_index].push(e); continue }
            for (let index = Math.max(0, e.day_index); index < Math.min(42, e.day_index + e.span_days); ++index) days[index].push(e)
        }
        return days
    }
    function eventsAt(index) { return eventsByDay[index] || [] }
    function allDayAt(index) { return eventsAt(index).filter(function(e) { return e.all_day && !calendar.editingEvent(e) }) }
    function openDay(index) {
        if (backend.openCalendarDay !== undefined) backend.openCalendarDay(isoAt(index))
        else { backend.setCalendarDate(isoAt(index)); backend.setCalendarView("day") }
    }
    function showEvent(event, sourceItem) { if (!backend.calendarReady || !backend.connected) return; if (editor.opened) { editor.contentItem.forceActiveFocus(); return } details.openAt(event, sourceItem || calendar) }
    function createEvent() { if (editor.opened) { editor.contentItem.forceActiveFocus(); return } editor.editEvent = ({}); editor.openForDate(backend.calendarDate) }
    function createAt(index, hour, endHour) {
        if (!backend.connected || !backend.calendarSources.some(function(s) { return s.writable })) return
        if (editor.opened && (editor.editEvent.event_id || editor.saving || editor.unknownOutcome || editor.staleRevision)) { editor.contentItem.forceActiveFocus(); return }
        if (!editor.opened) { editor.editEvent = ({}); editor.openForDate(isoAt(index)) }
        editor.setSlotRange(isoAt(index), hour, endHour)
    }
    function cancelRange() { rangeDrag = ({}) }
    function beginRange(index, y) {
        if (!backend.connected || !backend.calendarSources.some(function(s) { return s.writable }) || editor.opened) return false
        const start = Math.max(0, Math.min(23.5, Math.floor(y / hourHeight * 2) / 2))
        rangeDrag = {index: index, account: backend.account, from: backend.calendarFrom, view: backend.calendarView, start: start, end: start + .5, y: y, moved: false}
        return true
    }
    function moveRange(y) {
        if (rangeDrag.index === undefined) return
        rangeDrag = Object.assign({}, rangeDrag, {end: Math.max(0, Math.min(24, Math.round(y / hourHeight * 2) / 2)), moved: rangeDrag.moved || Math.abs(y - rangeDrag.y) > 3})
    }
    function finishRange() {
        const range = rangeDrag
        cancelRange()
        if (range.index === undefined || range.account !== backend.account || range.from !== backend.calendarFrom || range.view !== backend.calendarView || !backend.connected) return
        const low = Math.min(range.start, range.end), high = Math.max(range.start, range.end)
        if (!range.moved || high - low < .5) createAt(range.index, range.moved ? low : range.start)
        else createAt(range.index, low, high)
    }
    function hourText(hour) { const minutes = Math.round(hour * 60); return (Math.floor(minutes / 60) < 10 ? "0" : "") + Math.floor(minutes / 60) + ":" + (minutes % 60 < 10 ? "0" : "") + minutes % 60 }
    function sourceRule(event) { return event.busy_only || event.account !== backend.account ? Theme.inkAlpha(.45) : Theme.text }
    readonly property string accountName: (backend.accounts.find(function(a) { return a.id === backend.account }) || {}).name || backend.account
    function subtitle() {
        const count = agendaEvents.length, events = count + (count === 1 ? " event" : " events")
        return (backend.calendarView === "week" ? rangeTitle() : backend.calendarView === "day" && backend.calendarDate === backend.todayDate ? "Today · " + events : events) + " · " + accountName
    }
    function conferenceLabel(url) {
        const value = String(url || "").toLowerCase()
        return !value ? "" : value.indexOf("meet.google") >= 0 ? "Google Meet" : value.indexOf("teams") >= 0 ? "Teams" : value.indexOf("zoom") >= 0 ? "Zoom" : "Video call"
    }
    function people(event) { return (event.attendees || []).filter(function(a) { return !a.self }).map(function(a) { return a.name || a.email }).join(", ") }
    // Day-view block and list details: other account, place, call and guests.
    function eventSub(event) { return event.location || conferenceLabel(event.conference_url) || (event.account !== backend.account ? event.account_name || "" : "") }
    function agendaSub(event) { return [event.account !== backend.account ? event.account_name : "", event.location, conferenceLabel(event.conference_url), people(event)].filter(function(s) { return !!s }).join(" · ") }
    function eventPast(event) { return event.date < backend.todayDate || (event.date === backend.todayDate && !event.all_day && event.start_hour + event.duration_hours <= backend.nowHour) }
    function eventNow(event) { return event.date === backend.todayDate && !event.all_day && event.start_hour <= backend.nowHour && event.start_hour + event.duration_hours > backend.nowHour }
    function setCombined(on) { cancelRange(); details.close(); backend.setPreferences({calendar_account_visibility: on ? "all" : "current", selected_calendars: on ? null : backend.preferences.selected_calendars}) }
    function preserveGridPosition() {
        if (gridScroll.moving || gridScroll.height <= 0) return
        restoringGridPosition = true
        gridScroll.contentY = Math.max(0, Math.min(retainedTopHour * hourHeight, gridScroll.contentHeight - gridScroll.height))
        laidOutHourHeight = hourHeight
        laidOutGridHeight = gridScroll.height
        restoringGridPosition = false
    }
    function rememberGridPosition() {
        // Ignore Flickable's automatic clamp while geometry is changing. Keep
        // the intended time so a taller-then-shorter resize can restore it.
        if (!restoringGridPosition && laidOutHourHeight === hourHeight && laidOutGridHeight === gridScroll.height)
            retainedTopHour = Math.max(0, gridScroll.contentY) / hourHeight
    }
    Connections {
        target: backend
        function onAccountChanged() { calendar.cancelRange() }
        function onConnectedChanged() { if (!backend.connected) calendar.cancelRange() }
        function onEventsChanged() { if (calendar.rangeDrag.index !== undefined && (calendar.rangeDrag.from !== backend.calendarFrom || calendar.rangeDrag.view !== backend.calendarView)) calendar.cancelRange(); calendar.updateMailActivityContext() }
    }
    onHourHeightChanged: Qt.callLater(preserveGridPosition)
    onVisibleChanged: { updateMailActivityContext(); if (!visible) { cancelRange(); details.close(); datePicker.close(); moreMenu.close(); allDayOverflow.close() } }
    Shortcut { sequence: ShortcutRegistry.sequence("day",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("day") }
    Shortcut { sequence: ShortcutRegistry.sequence("week",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("week") }
    Shortcut { sequence: ShortcutRegistry.sequence("month",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("month") }
    Shortcut { sequence: ShortcutRegistry.sequence("agenda",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("agenda") }
    Shortcut { sequence: ShortcutRegistry.sequence("newEv",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: calendar.createEvent() }
    Shortcut { sequence: ShortcutRegistry.sequence("back",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.moveCalendar(-1) }
    Shortcut { sequence: ShortcutRegistry.sequence("fwd",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.moveCalendar(1) }
    Shortcut { sequence: ShortcutRegistry.sequence("today",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.showCalendarToday() }

    ColumnLayout {
        id: calendarColumn
        anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 24; spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.topMargin: 24; Layout.bottomMargin: 16; spacing: 12
            Item {
                Layout.fillWidth: true; implicitHeight: calendarTitle.implicitHeight
                Text { id: calendarTitle; objectName: "calendarTitle"; anchors.verticalCenter: parent.verticalCenter; text: backend.calendarView === "week" ? "Week " + calendar.weekNumber() : backend.calendarView === "agenda" ? "Agenda" : Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), backend.calendarView === "day" ? "dddd d MMMM" : "MMMM yyyy"); color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(22); font.weight: Font.Medium; font.letterSpacing: -0.44 }
                Text { objectName: "calendarSubtitle"; x: calendarTitle.width + 12; width: Math.max(0, parent.width - x); anchors.baseline: calendarTitle.baseline; text: calendar.subtitle(); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); visible: calendar.width > 1050; elide: Text.ElideRight; textFormat: Text.PlainText }
            }
            Item {
                implicitWidth: combinedRow.width + 8; implicitHeight: 30
                Row {
                    id: combinedRow; x: 4; anchors.verticalCenter: parent.verticalCenter; spacing: 9
                    FmSwitch {
                        objectName: "calendarCombinedToggle"; checked: calendar.combinedAccounts
                        implicitWidth: 28; implicitHeight: 16; anchors.verticalCenter: parent.verticalCenter
                        onClicked: calendar.setCombined(checked)
                        Accessible.name: "All calendars"
                    }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: "All calendars"; color: calendar.combinedAccounts || combinedHover.hovered ? Theme.text : Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5) }
                }
                HoverHandler { id: combinedHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: calendar.setCombined(!calendar.combinedAccounts) }
                FmToolTip { shown: combinedHover.hovered; text: calendar.combinedAccounts ? "Showing every account. Click for " + calendar.accountName + " only." : "Showing " + calendar.accountName + " only. Click to combine all accounts." }
            }
            FmSegmented {
                objectName: "calendarViewSegment"; model: ["Day", "Week", "Month"]; optionNames: ["calendarView_day", "calendarView_week", "calendarView_month"]
                currentIndex: ["day", "week", "month"].indexOf(backend.calendarView)
                onActivated: function(index) { backend.setCalendarView(["day", "week", "month"][index]); currentIndex = Qt.binding(function() { return ["day", "week", "month"].indexOf(backend.calendarView) }) }
                Accessible.name: "Calendar view"
            }
            Rectangle {
                id: navigation; implicitWidth: navigationRow.width + 2; implicitHeight: 30; radius: 5; color: "transparent"; border.color: Theme.controlBorder
                Row {
                    id: navigationRow; x: 1; y: 1; height: 28
                    FmButton { objectName: "calendarPrevious"; iconName: "previous"; iconSize: 12; width: 30; height: 28; radius: 4; foreground: Theme.inkAlpha(.7); tip: "Previous — Left"; background: Rectangle { radius: 4; color: parent.hovered ? Theme.surface : "transparent"; border.width: parent.activeFocus ? 1 : 0; border.color: Theme.focusBorder } onClicked: backend.moveCalendar(-1) }
                    Rectangle { width: Theme.ruleWidth; height: 28; color: Theme.controlBorder }
                    FmButton {
                        id: dateButton; objectName: "calendarDatePickerButton"; height: 28; leftPadding: 12; rightPadding: 12
                        width: Math.max(112, implicitWidth); font.pointSize: Theme.fontPoints(12.5); font.weight: Font.Normal
                        iconName: "down"; iconSize: 10; iconSpacing: 8; iconTrailing: true; iconColor: Theme.inkAlpha(.4)
                        foreground: backend.calendarDate === backend.todayDate ? Theme.text : Theme.inkAlpha(.85)
                        text: backend.calendarView === "week" ? Qt.formatDate(calendar.dateAt(0), calendar.dateAt(0).getMonth() === calendar.dateAt(6).getMonth() ? "d" : "d MMM") + " – " + Qt.formatDate(calendar.dateAt(6), "d MMM") : backend.calendarView === "month" ? Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "MMM yyyy") : backend.calendarDate === backend.todayDate ? "Today" : Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "ddd d MMM")
                        selected: datePicker.opened; tip: "Pick a date — T for today"
                        background: Rectangle { color: datePicker.opened || parent.hovered ? Theme.surface : "transparent"; border.width: parent.activeFocus ? 1 : 0; border.color: Theme.focusBorder }
                        onClicked: { datePicker.month = new Date(backend.calendarDate + "T12:00:00"); datePicker.opened ? datePicker.close() : datePicker.open() }
                    }
                    Rectangle { width: Theme.ruleWidth; height: 28; color: Theme.controlBorder }
                    FmButton { objectName: "calendarNext"; iconName: "next"; iconSize: 12; width: 30; height: 28; radius: 4; foreground: Theme.inkAlpha(.7); tip: "Next — Right"; background: Rectangle { radius: 4; color: parent.hovered ? Theme.surface : "transparent"; border.width: parent.activeFocus ? 1 : 0; border.color: Theme.focusBorder } onClicked: backend.moveCalendar(1) }
                }
            }
            FmButton { id: moreButton; objectName: "calendarMoreButton"; iconName: "more"; implicitWidth: 28; implicitHeight: 28; tip: "Calendar options and coverage"; onClicked: moreMenu.open() }
        }
        Rectangle {
            objectName: "calendarHiddenScopeNotice"
            visible: calendar.hiddenCalendarCount > 0 || calendar.accountScopeRestricted
            Layout.fillWidth: true; Layout.bottomMargin: 8
            implicitHeight: scopeNoticeRow.implicitHeight + 14
            radius: 5; color: Theme.surface; border.color: Theme.controlBorder
            RowLayout {
                id: scopeNoticeRow; anchors.fill: parent; anchors.margins: 7; spacing: 10
                Text {
                    Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    text: calendar.hiddenCalendarCount > 0
                        ? "Showing " + calendar.selectedCalendarCount + " of " + calendar.permittedCalendarCount + " calendars. Events on hidden calendars will not appear here."
                        : "Showing the current account only. Events on other accounts will not appear here."
                    font.pointSize: Theme.fontPoints(11); color: Theme.secondary
                }
                FmButton {
                    objectName: "calendarShowAllFromNotice"; text: "Show all"; outlined: true
                    onClicked: backend.setPreferences({selected_calendars:null, calendar_account_visibility:"all"})
                }
            }
        }
        Text { visible: !backend.calendarReady && (!calendar.calendarLoading || calendar.slowLoad); text: backend.calendarCoverage; Layout.fillWidth: true; Layout.bottomMargin: 8; font.pointSize: Theme.fontPoints(11); color: Theme.subtle; wrapMode: Text.Wrap }
        RowLayout {
            id: dayHeaderRow
            visible: backend.calendarView === "day" || backend.calendarView === "week"
            Layout.fillHeight: false; Layout.maximumHeight: implicitHeight
            Layout.fillWidth: true; Layout.leftMargin: 56; Layout.rightMargin: backend.calendarView === "day" && calendar.width > 850 ? 352 : 0; spacing: 0
            Repeater { model: calendar.timeGrid ? calendar.dayCount : 0
                ColumnLayout {
                    id: dayHeader; required property int index; property var allDay: calendar.allDayAt(index)
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1; spacing: 0
                    RowLayout { Layout.fillWidth: true; spacing: 8
                        FmButton {
                            Layout.fillWidth: true; implicitHeight: Math.round(Theme.fontSize(20) * 1.4); leftPadding: 10; rightPadding: 0; topPadding: 0; bottomPadding: 0
                            onClicked: calendar.openDay(dayHeader.index); tip: "Open day"
                            background: Rectangle { color: "transparent"; radius: 4; border.width: parent.activeFocus ? 1 : 0; border.color: Theme.focusBorder }
                            contentItem: Item {
                                implicitHeight: dayNumber.implicitHeight
                                Text { id: dayNumber; anchors.verticalCenter: parent.verticalCenter; text: Qt.formatDate(calendar.dateAt(dayHeader.index), "d"); color: calendar.isoAt(dayHeader.index) === backend.todayDate ? Theme.accent : Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(20); font.weight: Font.Medium; font.letterSpacing: -0.4 }
                                // Week names centre on the date; the day view shares its baseline.
                                Text {
                                    x: dayNumber.width + 8; text: Qt.formatDate(calendar.dateAt(dayHeader.index), calendar.dayCount === 1 ? "dddd" : "ddd"); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
                                    y: calendar.dayCount === 1 ? dayNumber.y + dayNumber.baselineOffset - baselineOffset : (parent.height - height) / 2
                                }
                            }
                        }
                        Text { visible: backend.calendarView === "day"; text: "Click an empty slot to add an event"; color: Theme.inkAlpha(.35); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; horizontalAlignment: Text.AlignRight; Layout.fillWidth: true }
                        CalendarMailCount {
                            objectName: "calendarMailCount-" + calendar.isoAt(dayHeader.index)
                            Layout.rightMargin: 6
                            date: calendar.isoAt(dayHeader.index); row: calendar.mailDays[date]
                            accountName: calendar.accountName
                            compact: width < 165
                            onClicked: calendar.openMailRequested()
                        }
                    }
                    Flow {
                        visible: calendar.dayCount === 1; Layout.fillWidth: true; Layout.topMargin: 6; Layout.leftMargin: 10; spacing: 6
                        Repeater { model: calendar.dayCount === 1 ? dayHeader.allDay : []
                            Rectangle {
                                id: allDayChip; required property var modelData
                                objectName: "calendarAllDay-" + (modelData.event_id || modelData.id)
                                width: Math.min(allDayChipText.implicitWidth + 22, parent.width); height: 22; radius: 3
                                color: allDayChipMouse.containsMouse ? Theme.hover : Theme.selected
                                activeFocusOnTab: visible && backend.calendarReady
                                Keys.onReturnPressed: calendar.showEvent(modelData, allDayChip)
                                Keys.onSpacePressed: calendar.showEvent(modelData, allDayChip)
                                border.width: activeFocus ? 1 : 0; border.color: Theme.accent
                                Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                                Accessible.onPressAction: calendar.showEvent(modelData, allDayChip)
                                Rectangle { width: 3; height: parent.height; color: parent.color }
                                Rectangle { width: 2; height: parent.height; color: calendar.sourceRule(parent.modelData) }
                                Text { id: allDayChipText; x: 12; width: parent.width - 22; height: parent.height; text: "All day · " + allDayChip.modelData.title; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; textFormat: Text.PlainText }
                                MouseArea { id: allDayChipMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: calendar.showEvent(parent.modelData, parent) }
                            }
                        }
                    }
                    Repeater { model: calendar.dayCount === 1 ? [] : dayHeader.allDay.slice(0, 2)
                        Rectangle {
                            id: allDayBox
                            required property var modelData; Layout.fillWidth: true; implicitHeight: 22; Layout.topMargin: 6; Layout.leftMargin: 10; Layout.rightMargin: 6; color: allDayMouse.containsMouse ? Theme.hover : Theme.selected; radius: 3
                            objectName: "calendarAllDay-" + (modelData.event_id || modelData.id)
                            activeFocusOnTab: visible && backend.calendarReady
                            Keys.onReturnPressed: calendar.showEvent(modelData, allDayBox)
                            Keys.onSpacePressed: calendar.showEvent(modelData, allDayBox)
                            border.width: activeFocus ? 1 : 0; border.color: Theme.accent
                            Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                            Accessible.onPressAction: calendar.showEvent(modelData, allDayBox)
                            Rectangle { width: 3; height: parent.height; color: parent.color }
                            Rectangle { width: 2; height: parent.height; color: calendar.sourceRule(parent.modelData) }
                            Text { anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 8; text: modelData.title; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; textFormat: Text.PlainText }
                            MouseArea { id: allDayMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: calendar.showEvent(parent.modelData, parent) }
                            FmToolTip { shown: allDayMouse.containsMouse; text: "All day · " + allDayBox.modelData.title }
                        }
                    }
                    Repeater {
                        model: calendar.previewRows.filter(function(event) { return event.all_day && event.day_index <= dayHeader.index && event.day_index + event.span_days > dayHeader.index })
                        Rectangle {
                            required property var modelData; objectName: "calendarAllDayPreview-" + dayHeader.index
                            Layout.fillWidth: true; implicitHeight: 22; Layout.topMargin: 6; Layout.leftMargin: 10; Layout.rightMargin: 6; radius: 3; color: Theme.accentAlpha(0.125); border.color: Theme.accent
                            Text { anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 4; text: modelData.title; color: Theme.accent; font.pointSize: Theme.fontPoints(11); verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; textFormat: Text.PlainText }
                        }
                    }
                    FmButton { visible: calendar.dayCount > 1 && dayHeader.allDay.length > 2; text: "+ " + (dayHeader.allDay.length - 2) + " all day"; implicitHeight: 20; Layout.topMargin: 6; Layout.leftMargin: 4; font.pointSize: Theme.fontPoints(11); onClicked: { allDayOverflow.eventRows = dayHeader.allDay; allDayOverflow.open() } }
                    Item { Layout.fillHeight: true }
                    Rectangle { Layout.fillWidth: true; Layout.topMargin: calendar.dayCount === 1 && dayHeader.allDay.length === 0 ? 16 : 10; implicitHeight: 1; color: calendar.isoAt(dayHeader.index) === backend.todayDate ? Theme.accent : Theme.line }
                }
            }
        }
        RowLayout {
            id: gridRow
            visible: backend.calendarView === "day" || backend.calendarView === "week"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; spacing: 32
            Flickable {
                id: gridScroll; objectName: "calendarGrid"; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: width; contentHeight: 24 * calendar.hourHeight + 10; boundsBehavior: Flickable.StopAtBounds
                Component.onCompleted: Qt.callLater(calendar.preserveGridPosition)
                onHeightChanged: Qt.callLater(calendar.preserveGridPosition)
                onContentYChanged: calendar.rememberGridPosition()
                onMovementEnded: { calendar.retainedTopHour = Math.max(0, contentY) / calendar.hourHeight; backend.remember("calendarY", contentY); Qt.callLater(calendar.preserveGridPosition) }
                ScrollBar.vertical: FmScrollBar {}
                Repeater { model: 25
                    Text { required property int index; objectName: "calendarHour-" + index; x: 0; y: Math.max(0, index * calendar.hourHeight - (index === 24 ? height : index === 0 ? 0 : height / 2)); text: (index < 10 ? "0" : "") + index + ":00"; color: Theme.inkAlpha(index >= calendar.workingStart && index <= calendar.workingEnd ? .5 : .3); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.features: {"tnum": 1} }
                }
                Repeater {
                    model: [calendar.workingStart, calendar.workingEnd]
                    Item {
                        required property int index; required property real modelData
                        objectName: "calendarWorkingHour-" + index
                        x: 42; y: modelData * calendar.hourHeight - 4; width: 8; height: 8
                        FmIcon { name: "down"; size: 8; color: Theme.inkAlpha(.35); rotation: parent.index === 0 ? 0 : 180 }
                        HoverHandler { id: workingHover }
                        FmToolTip { shown: workingHover.hovered; text: "Working hours " + calendar.hourText(calendar.workingStart) + " – " + calendar.hourText(calendar.workingEnd) }
                    }
                }
                Item {
                    id: grid; x: 56; width: gridScroll.width - 56; height: 24 * calendar.hourHeight; property real dayWidth: width / calendar.dayCount
                    Repeater { model: calendar.timeGrid ? calendar.dayCount : 0
                        Rectangle {
                            required property int index; x: index * grid.dayWidth; width: grid.dayWidth; height: grid.height; color: calendar.isoAt(index) === backend.todayDate ? Theme.bg1 : "transparent"
                            Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: Theme.ruleWidth; height: parent.height; color: Theme.line }
                            MouseArea {
                                objectName: "calendarSlot-" + parent.index
                                anchors.fill: parent; cursorShape: Qt.CrossCursor; preventStealing: calendar.rangeDrag.index !== undefined
                                onPressed: function(mouse) { if (!calendar.beginRange(parent.index, mouse.y)) mouse.accepted = false }
                                onPositionChanged: function(mouse) { if (pressed) calendar.moveRange(mouse.y) }
                                onReleased: calendar.finishRange()
                                onCanceled: calendar.cancelRange()
                            }
                            Rectangle { visible: calendar.isoAt(index) === backend.todayDate; x: Theme.snapX(parent, 0); y: Theme.snapY(parent, backend.nowHour * calendar.hourHeight); z: 2; width: parent.width; height: Theme.ruleWidth; color: Theme.accent
                                Canvas { x: -4; y: -4; width: 8; height: 9; onPaint: { const c = getContext("2d"); c.reset(); c.fillStyle = Theme.accent; c.beginPath(); c.moveTo(0, 0); c.lineTo(8, 4.5); c.lineTo(0, 9); c.closePath(); c.fill() } }
                            }
                        }
                    }
                    Repeater { model: 24
                        Rectangle { required property int index; x: Theme.snapX(parent, 0); y: Theme.snapY(parent, index * calendar.hourHeight); width: grid.width; height: Theme.ruleWidth; color: Theme.bg5 }
                    }
                    Repeater { model: 24
                        Rectangle { required property int index; x: Theme.snapX(parent, 0); y: Theme.snapY(parent, (index + .5) * calendar.hourHeight); width: grid.width; height: Theme.ruleWidth; color: Theme.inkAlpha(.035) }
                    }
                    Rectangle {
                        objectName: "calendarRangePreview"
                        readonly property real low: Math.min(calendar.rangeDrag.start || 0, calendar.rangeDrag.end || 0)
                        readonly property real high: Math.max(calendar.rangeDrag.start || 0, calendar.rangeDrag.end || 0)
                        visible: calendar.rangeDrag.index !== undefined
                        x: (calendar.rangeDrag.index || 0) * grid.dayWidth + 4; y: low * calendar.hourHeight
                        width: grid.dayWidth - 10; height: Math.max(calendar.hourHeight / 2, (high - low) * calendar.hourHeight)
                        color: Theme.accentAlpha(.1); radius: 3
                        // Dashes exist only while a range is being dragged.
                        Repeater { model: parent.visible ? Math.max(0, Math.ceil(parent.width / 8)) : 0; Rectangle { required property int index; x: Theme.snapX(parent, index * 8); y: Theme.snapY(parent, 0); width: Math.max(0, Math.min(4, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                        Repeater { model: parent.visible ? Math.max(0, Math.ceil(parent.width / 8)) : 0; Rectangle { required property int index; x: Theme.snapX(parent, index * 8); y: Theme.snapY(parent, parent.height - 1); width: Math.max(0, Math.min(4, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                        Rectangle { width: 2; height: parent.height; color: Theme.accent }
                        Text { anchors.fill: parent; anchors.margins: 4; text: calendar.hourText(parent.low) + " – " + calendar.hourText(parent.high); color: Theme.accent; font.pointSize: Theme.fontPoints(11); elide: Text.ElideRight }
                    }
                    // A count model keeps event boxes when navigation shows the same number of events.
                    Repeater { model: calendar.timeGrid ? calendar.visibleEvents.length : 0
                        Rectangle {
                            id: eventBox; required property int index; readonly property var modelData: calendar.visibleEvents[index] || ({})
                            objectName: "calendarEvent-" + (modelData.event_id || modelData.id)
                            activeFocusOnTab: visible && backend.calendarReady
                            Keys.onReturnPressed: calendar.showEvent(modelData, eventBox)
                            Keys.onSpacePressed: calendar.showEvent(modelData, eventBox)
                            border.width: activeFocus ? 1 : 0; border.color: Theme.accent
                            visible: !modelData.all_day && modelData.day_index >= 0 && modelData.day_index < calendar.dayCount && !calendar.editingEvent(modelData)
                            readonly property real inset: calendar.dayCount === 1 ? 4 : 3
                            readonly property real span: grid.dayWidth - (calendar.dayCount === 1 ? 10 : 7)
                            x: modelData.day_index * grid.dayWidth + inset + (modelData.column || 0) * span / (modelData.columns || 1); y: modelData.start_hour * calendar.hourHeight
                            width: span / (modelData.columns || 1) - ((modelData.columns || 1) > 1 ? 3 : 0); height: Math.max(20, modelData.duration_hours * calendar.hourHeight - 1)
                            radius: calendar.dayCount === 1 ? 4 : 3; color: eventMouse.containsMouse ? Theme.hover : Theme.selected; clip: true
                            Rectangle { width: 4; height: parent.height; color: parent.color }
                            Rectangle { width: 2; height: parent.height; color: calendar.sourceRule(eventBox.modelData) }
                            Row {
                                anchors.top: parent.top; anchors.topMargin: 3; anchors.left: parent.left; anchors.leftMargin: calendar.dayCount === 1 ? 14 : 9; anchors.right: parent.right; anchors.rightMargin: accountBadge.visible ? 22 : 7; spacing: calendar.dayCount === 1 ? 14 : 6
                                Text { id: eventTime; visible: eventBox.width >= 64 || calendar.dayCount === 1; y: eventTitle.baselineOffset - baselineOffset; text: eventBox.modelData.time_display + (calendar.dayCount === 1 ? " – " + eventBox.modelData.end_display : ""); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(calendar.dayCount === 1 ? 11.5 : 10.5); font.features: {"tnum": 1} }
                                Text { id: eventTitle; text: eventBox.modelData.title; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(calendar.dayCount === 1 ? 13 : 12); font.weight: Font.Medium; width: Math.min(implicitWidth, Math.max(0, parent.width - (eventTime.visible ? eventTime.width + parent.spacing : 0))); elide: Text.ElideRight }
                                Text { visible: calendar.dayCount === 1 && text.length > 0; y: eventTitle.baselineOffset - baselineOffset; text: calendar.eventSub(eventBox.modelData); width: Math.max(0, parent.width - eventTime.width - eventTitle.width - 2 * parent.spacing); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; textFormat: Text.PlainText }
                            }
                            MouseArea { id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { eventBox.forceActiveFocus(); calendar.showEvent(eventBox.modelData, eventBox) } }
                            FmToolTip { shown: eventMouse.containsMouse; text: eventBox.modelData.title + " · " + eventBox.modelData.start_display + "\n" + eventBox.modelData.account_name + " / " + eventBox.modelData.calendar_name }
                            AccountBadge {
                                id: accountBadge; diameter: 14; idleColor: Theme.bg6; idleText: Theme.inkAlpha(.65)
                                anchors.top: parent.top; anchors.right: parent.right; anchors.topMargin: 3; anchors.rightMargin: 4
                                accountName: eventBox.modelData.account_name || ""
                                selected: eventBox.modelData.account === backend.account
                                visible: (eventBox.modelData.columns || 1) === 1
                            }
                            Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                            Accessible.onPressAction: calendar.showEvent(modelData, eventBox)
                        }
                    }
                    Repeater {
                        model: calendar.previewRows.filter(function(event) { return !event.all_day })
                        Rectangle {
                            required property var modelData
                            objectName: "calendarEditorPreview-" + modelData.day_index
                            x: modelData.day_index * grid.dayWidth + 3; y: modelData.start_hour * calendar.hourHeight; width: grid.dayWidth - 7
                            height: Math.max(20, modelData.duration_hours * calendar.hourHeight); color: Theme.accentAlpha(.125); radius: 3
                            Repeater { model: Math.max(0, Math.ceil(parent.width / 10)); Rectangle { required property int index; x: Theme.snapX(parent, index * 10); y: Theme.snapY(parent, 0); width: Math.max(0, Math.min(5, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                            Repeater { model: Math.max(0, Math.ceil(parent.width / 10)); Rectangle { required property int index; x: Theme.snapX(parent, index * 10); y: Theme.snapY(parent, parent.height - 1); width: Math.max(0, Math.min(5, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                            Rectangle { width: 2; height: parent.height; color: Theme.accent }
                            Text { anchors.fill: parent; anchors.margins: 7; text: editor.preview.title; color: Theme.accent; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; textFormat: Text.PlainText }
                        }
                    }
                }
            }
            Item { visible: backend.calendarView === "day" && calendar.width > 850; Layout.preferredWidth: 320; Layout.fillHeight: true }
        }
        GridLayout {
            visible: backend.calendarView === "month"; Layout.fillWidth: true; columns: 7; columnSpacing: 0
            Repeater { model: 7; Text { required property int index; Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.leftMargin: 10; Layout.bottomMargin: 8; text: Qt.formatDate(calendar.dateAt(index), "ddd"); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) } }
        }
        GridLayout {
            id: monthGrid
            visible: backend.calendarView === "month"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; columns: 7; columnSpacing: 0; rowSpacing: 0
            Repeater { model: 42
                Rectangle {
                    id: monthCell; objectName: "calendarMonthCell-" + day.iso; required property int index; visible: index < calendar.monthCells
                    readonly property var day: calendar.monthDates[index] || calendar.emptyDay
                    property var dayEvents: calendar.monthEvents[index] || []; property bool inMonth: day.date.getMonth() === calendar.monthNumber; property int shown: Math.min(3, Math.max(0, Math.floor((height - 50) / 18) - (dayEvents.length * 18 > height - 50 ? 1 : 0)))
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1; Layout.preferredHeight: 1; clip: true
                    color: day.iso === backend.todayDate || monthHover.hovered ? Theme.bg1 : "transparent"
                    Rectangle { visible: monthCell.index < 7; width: parent.width; height: Theme.ruleWidth; color: Theme.line }
                    Rectangle { visible: monthCell.index % 7 === 0; width: Theme.ruleWidth; height: parent.height; color: Theme.line }
                    Rectangle { x: parent.width - width; width: Theme.ruleWidth; height: parent.height; color: Theme.line }
                    Rectangle { y: parent.height - height; width: parent.width; height: Theme.ruleWidth; color: Theme.line }
                    HoverHandler { id: monthHover }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: calendar.openDay(monthCell.index) }
                    FmToolTip { shown: monthHover.hovered && !monthDate.hovered; text: shown ? "Open " + Qt.formatDate(monthCell.day.date, "dddd d MMMM") : "" }
                    Column { x: 8; y: 6; width: parent.width - 16; spacing: 3
                        Item { width: parent.width; height: Math.round(Theme.fontSize(13) * 1.4) + 6
                            FmButton { id: monthDate; objectName: "calendarMonthDate-" + monthCell.index; text: monthCell.day.iso ? monthCell.day.date.getDate() : ""; height: Math.round(Theme.fontSize(13) * 1.4) + 4; leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0; textAlignment: Text.AlignLeft; font.pointSize: Theme.fontPoints(13); font.weight: Font.Medium; foreground: monthCell.day.iso === backend.todayDate ? Theme.accent : monthCell.inMonth ? Theme.text : Theme.inkAlpha(.3); background: Item {} onClicked: calendar.openDay(monthCell.index) }
                        // Only past and current days with counts create a badge.
                        Loader {
                            active: !!calendar.mailDays[monthCell.day.iso] && monthCell.day.iso <= backend.todayDate
                            anchors.right: parent.right; anchors.verticalCenter: monthDate.verticalCenter
                            sourceComponent: CalendarMailCount {
                                objectName: "calendarMonthMailCount-" + monthCell.day.iso
                                date: monthCell.day.iso; row: calendar.mailDays[date]
                                accountName: calendar.accountName; compact: true; textSize: 10.5
                                opacity: monthCell.inMonth ? 1 : 0.6
                                onClicked: calendar.openMailRequested()
                            }
                        }
                        }
                        // At most three rows fit. A count model keeps rows while the grid lays out and on navigation.
                        Repeater { model: Math.min(3, monthCell.dayEvents.length)
                            Rectangle {
                                id: monthEvent; required property int index; readonly property var modelData: monthCell.dayEvents[index] || ({}); visible: index < monthCell.shown; width: monthCell.width - 16; height: Math.round(Theme.fontSize(11.5) * 1.3); color: "transparent"
                                Rectangle { width: 2; height: parent.height; color: monthCell.inMonth ? calendar.sourceRule(parent.modelData) : Theme.line3 }
                                FmButton {
                                    objectName:"calendarMonthEvent-"+(parent.modelData.event_id||parent.modelData.id); anchors.fill: parent; leftPadding: 8; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                    tip: (monthEvent.modelData.all_day ? "All day" : monthEvent.modelData.time_display + " – " + monthEvent.modelData.end_display) + " · " + monthEvent.modelData.title
                                    Accessible.name: monthEvent.modelData.title
                                    background: Item {}
                                    contentItem: Row {
                                        spacing: 4
                                        Text { id: monthEventTime; visible: !monthEvent.modelData.all_day; text: monthEvent.modelData.time_display; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); font.features: {"tnum": 1}; anchors.verticalCenter: parent.verticalCenter }
                                        Text { text: monthEvent.modelData.title; width: Math.max(0, monthEvent.width - 8 - (monthEventTime.visible ? monthEventTime.width + 4 : 0)); color: monthCell.inMonth ? Theme.text : Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); elide: Text.ElideRight; textFormat: Text.PlainText; anchors.verticalCenter: parent.verticalCenter }
                                    }
                                    onClicked: calendar.showEvent(monthEvent.modelData, monthEvent)
                                }
                            }
                        }
                        Loader {
                            active: monthCell.dayEvents.length > monthCell.shown; visible: active
                            sourceComponent: FmButton { text: "+ " + (monthCell.dayEvents.length - monthCell.shown) + " more"; height: Math.round(Theme.fontSize(11) * 1.4); leftPadding: 8; rightPadding: 0; topPadding: 0; bottomPadding: 0; textAlignment: Text.AlignLeft; foreground: hovered ? Theme.text : Theme.inkAlpha(.45); font.pointSize: Theme.fontPoints(11); background: Item {} onClicked: calendar.openDay(monthCell.index) }
                        }
                    }
                }
            }
        }
        ListView {
            id: agenda; objectName: "calendarAgenda"; visible: backend.calendarView === "agenda"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; model: visible ? calendar.agendaEvents : []; clip: true
            delegate: ItemDelegate {
                required property var modelData; objectName: (modelData.recurring_selector || modelData.occurrence_selector) ? "calendarAgendaOccurrence-" + modelData.id : "calendarAgendaEvent-" + modelData.account + "-" + modelData.event_id; width: agenda.width; height: 70; onClicked: calendar.showEvent(modelData, this)
                background: Rectangle { color: parent.hovered ? Theme.surface : "transparent"; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
                contentItem: RowLayout { spacing: 18
                    Text { text: modelData.start_display; color: Theme.subtle; font.pointSize: Theme.fontPoints(12); Layout.preferredWidth: 200 }
                    Column { Layout.fillWidth: true; spacing: 4
                        Text { text: modelData.title; color: Theme.text; font.pointSize: Theme.fontPoints(13); textFormat: Text.PlainText }
                        Text { text: modelData.account_name + " / " + modelData.calendar_name + (modelData.location ? " · " + modelData.location : ""); color: Theme.subtle; font.pointSize: Theme.fontPoints(11); textFormat: Text.PlainText }
                    }
                }
            }
            Text { anchors.centerIn: parent; visible: agenda.count === 0; text: backend.calendarReady ? "Nothing scheduled in this range." : backend.calendarCoverage; color: Theme.subtle; font.pointSize: Theme.fontPoints(13) }
        }
    }
    ListView {
        id: dayAgenda; objectName: "calendarDayAgenda"
        visible: backend.calendarView === "day" && calendar.width > 850
        x: calendar.width - 24 - width; y: calendarColumn.y + dayHeaderRow.y + 38; width: 320
        height: Math.max(0, calendarColumn.y + gridRow.y + gridRow.height - y); clip: true; model: visible ? calendar.agendaEvents : []
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: FmScrollBar {}
        delegate: ItemDelegate {
            id: dayAgendaRow; required property var modelData
            readonly property bool past: calendar.eventPast(modelData)
            width: ListView.view.width; topPadding: 12; bottomPadding: 12; leftPadding: 0; rightPadding: 0
            implicitHeight: dayAgendaText.implicitHeight + 25
            onClicked: calendar.showEvent(modelData, this)
            background: Rectangle { color: dayAgendaRow.hovered ? Theme.surface : "transparent"; border.width: dayAgendaRow.activeFocus ? 1 : 0; border.color: Theme.focusBorder; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
            contentItem: Item {
                Text { width: 92; text: dayAgendaRow.modelData.all_day ? "All day" : dayAgendaRow.modelData.time_display + " – " + dayAgendaRow.modelData.end_display; color: calendar.eventNow(dayAgendaRow.modelData) ? Theme.accent : Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); font.features: {"tnum": 1}; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12.5) * 1.4); elide: Text.ElideRight }
                Column {
                    id: dayAgendaText; x: 106; width: parent.width - x; spacing: 2
                    Text { width: parent.width; text: dayAgendaRow.modelData.title; textFormat: Text.PlainText; color: dayAgendaRow.past ? Theme.inkAlpha(.45) : Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5); font.weight: Font.Medium; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(13.5) * 1.4); elide: Text.ElideRight }
                    Text { width: parent.width; visible: text.length > 0; text: calendar.agendaSub(dayAgendaRow.modelData); textFormat: Text.PlainText; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12) * 1.4); elide: Text.ElideRight }
                }
            }
        }
        Text { visible: dayAgenda.count === 0; y: 12; text: "Nothing scheduled."; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
    }
    Popup {
        id: datePicker; parent: dateButton; y: 36; x: parent.width - width + 31; width: 272; padding: 12
        property date month: new Date()
        property int weekStart: backend.preferences.week_start === "sunday" ? 0 : 1
        property int offset: (new Date(month.getFullYear(), month.getMonth(), 1).getDay() - weekStart + 7) % 7
        property int days: Math.ceil((offset + new Date(month.getFullYear(), month.getMonth() + 1, 0).getDate()) / 7) * 7
        property int hoveredCell: -1
        function dateFor(index) { return new Date(month.getFullYear(), month.getMonth(), 1 + index - offset, 12) }
        function select(date) { backend.setCalendarDate(Qt.formatDate(date, "yyyy-MM-dd")); close() }
        modal: false; focus: true; closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 8
            RowLayout {
                Layout.fillWidth: true
                FmButton { iconName: "previous"; implicitWidth: 26; implicitHeight: 26; onClicked: datePicker.month = new Date(datePicker.month.getFullYear() - (backend.calendarView === "month" ? 1 : 0), datePicker.month.getMonth() - (backend.calendarView === "month" ? 0 : 1), 1) }
                Text { text: Qt.formatDate(datePicker.month, backend.calendarView === "month" ? "yyyy" : "MMMM yyyy"); color: Theme.text; font.pointSize: Theme.fontPoints(13); font.weight: Font.Medium; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter }
                FmButton { iconName: "next"; implicitWidth: 26; implicitHeight: 26; onClicked: datePicker.month = new Date(datePicker.month.getFullYear() + (backend.calendarView === "month" ? 1 : 0), datePicker.month.getMonth() + (backend.calendarView === "month" ? 0 : 1), 1) }
            }
            GridLayout {
                visible: backend.calendarView !== "month"; Layout.fillWidth: true; columns: 7; rowSpacing: 2; columnSpacing: 0
                Repeater { model: 7; Text { required property int index; text: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][(index + datePicker.weekStart) % 7]; color: Theme.subtle; font.pointSize: Theme.fontPoints(10); Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter } }
                Repeater { model: datePicker.visible ? datePicker.days : 0
                    FmButton {
                        required property int index; property date day: datePicker.dateFor(index); property string iso: Qt.formatDate(day, "yyyy-MM-dd")
                        text: day.getDate(); Layout.fillWidth: true; Layout.preferredWidth: 1; implicitHeight: 30; leftPadding: 0; rightPadding: 0; font.pointSize: Theme.fontPoints(12)
                        selected: backend.calendarView === "week" ? iso >= backend.calendarFrom && iso <= calendar.isoAt(6) : iso === backend.calendarDate
                        foreground: selected ? Theme.canvas : iso === backend.todayDate ? Theme.accent : day.getMonth() === datePicker.month.getMonth() ? Theme.text : Theme.inkAlpha(.5)
                        background: Rectangle { radius: backend.calendarView === "week" ? 0 : 5; color: parent.selected ? Theme.text : parent.hovered || (backend.calendarView === "week" && Math.floor(parent.index / 7) === Math.floor(datePicker.hoveredCell / 7)) ? Theme.hover : "transparent" }
                        onHoveredChanged: if (hovered) datePicker.hoveredCell = index
                        onClicked: datePicker.select(day)
                    }
                }
            }
            GridLayout {
                visible: backend.calendarView === "month"; Layout.fillWidth: true; columns: 3; columnSpacing: 4; rowSpacing: 4
                Repeater { model: datePicker.visible ? 12 : 0
                    FmButton { required property int index; text: Qt.formatDate(new Date(datePicker.month.getFullYear(), index, 1), "MMM"); Layout.fillWidth: true; implicitHeight: 34; selected: index === new Date(backend.calendarDate + "T12:00:00").getMonth() && datePicker.month.getFullYear() === new Date(backend.calendarDate + "T12:00:00").getFullYear(); onClicked: datePicker.select(new Date(datePicker.month.getFullYear(), index, 1, 12)) }
                }
            }
            Text { visible: backend.calendarView === "week"; text: "Pick any day — the whole week opens"; color: Theme.subtle; font.pointSize: Theme.fontPoints(10); Layout.alignment: Qt.AlignHCenter }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            RowLayout { Layout.fillWidth: true
                FmButton { text: "Today"; foreground: Theme.accent; implicitHeight: 24; font.pointSize: Theme.fontPoints(12); onClicked: { backend.showCalendarToday(); datePicker.close() } }
                Text { text: backend.preferences.calendar_display_time_zone || "Time zone unavailable"; color: Theme.subtle; font.pointSize: Theme.fontPoints(10); Layout.fillWidth: true; horizontalAlignment: Text.AlignRight }
            }
        }
    }
    Popup {
        id: moreMenu; parent: moreButton; x: parent.width - width; y: 34; width: 255; padding: 8; focus: true
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 4
            FmButton { objectName: "calendarAgendaButton"; text: "Agenda"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { backend.setCalendarView("agenda"); moreMenu.close() } }
            FmButton { objectName: "calendarActivityButton"; text: backend.syntheticProfile ? "Local calendar activity" : "Calendar activity"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { moreMenu.close(); operationHistory.open() } }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            Text { text: "Combined · " + (backend.preferences.calendar_display_time_zone || "Time zone unavailable") + "\n" + backend.calendarCoverage; color: Theme.subtle; font.pointSize: Theme.fontPoints(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }
    EventDetails { id: details; onEditRequested: function(event) { editor.editEvent = event; editor.openForDate(event.start.slice(0, 10)) } }
    EventEditor { id: editor }
    CalendarOperations { id: operationHistory }
    CalendarEventList { id: allDayOverflow; title: "All-day events"; available: backend.calendarReady && backend.connected; onEventSelected: function(event) { calendar.showEvent(event) } }
}
