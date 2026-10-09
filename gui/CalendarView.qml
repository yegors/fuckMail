// The Calendar workspace: Day, Week, Month and Agenda views, the header with
// navigation and date picker, and the event card, editor and activity popups.
// Driven by `backend` (events, calendarView, calendarDate, preferences).
import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Shapes

Item {
    id: calendar
    // Maybe: 5px ink bands every 10px at 135° over the block's fill.
    component MaybeBands: Canvas {
        onPaint: {
            const c = getContext("2d"); c.reset(); c.fillStyle = Theme.inkAlpha(.045); c.rotate(Math.PI / 4)
            const d = width + height
            for (let s = -d; s < d; s += 10) c.fillRect(s + 5, -d, 5, 2 * d)
        }
        onWidthChanged: requestPaint(); onHeightChanged: requestPaint(); onVisibleChanged: if (visible) requestPaint()
    }
    // Not answered: a 1px dashed outline.
    component DashedOutline: Shape {
        id: outline
        property color color; property real radius: 5
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            strokeColor: outline.color; strokeWidth: 1; fillColor: "transparent"; strokeStyle: ShapePath.DashLine; dashPattern: [3, 3]
            PathRectangle { x: .5; y: .5; width: Math.max(0, outline.width - 1); height: Math.max(0, outline.height - 1); radius: outline.radius }
        }
    }
    // A month row's 2px edge: solid when accepted, dashed for Maybe, dotted
    // when not answered.
    component ChipEdge: Item {
        id: edge
        property string style: "solid"; property color color
        width: 2
        Rectangle { visible: edge.style === "solid"; anchors.fill: parent; color: edge.color }
        Repeater {
            model: edge.style === "solid" ? 0 : Math.ceil(edge.height / (edge.style === "dashed" ? 6 : 4))
            Rectangle { required property int index; width: edge.width; y: index * (edge.style === "dashed" ? 6 : 4); height: Math.min(edge.style === "dashed" ? 4 : 2, edge.height - y); color: edge.color }
        }
    }
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
    readonly property var accountEvents: combinedAccounts ? backend.events : backend.events.filter(function(e) { return e.account === backend.account })
    // Text in the search box filters every view to its matches.
    property var visibleEvents: backend.filterCalendarEvents === undefined ? accountEvents : backend.filterCalendarEvents(accountEvents, backend.calendarSearchTerms)
    readonly property real workingStart: backend.preferences.timeline_start_hour === undefined ? 0 : backend.preferences.timeline_start_hour
    readonly property real workingEnd: backend.preferences.timeline_end_hour === undefined ? 24 : backend.preferences.timeline_end_hour
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
    readonly property bool popupOpen: editor.opened || details.opened || details.viewOpen || operationHistory.opened || datePicker.opened || moreMenu.opened || allDayOverflow.opened
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
    // ISO week of a row's midweek day, as the Week title and Month axis show it.
    function isoWeek(date) { let utc = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())); utc.setUTCDate(utc.getUTCDate() + 4 - (utc.getUTCDay() || 7)); return Math.ceil((((utc - new Date(Date.UTC(utc.getUTCFullYear(), 0, 1))) / 86400000) + 1) / 7) }
    function weekNumber() { return isoWeek(dateAt(3)) }
    function openWeek(date) {
        if (backend.openCalendarWeek !== undefined) backend.openCalendarWeek(date)
        else { backend.setCalendarDate(date); backend.setCalendarView("week") }
    }
    // The band above the grid is 36px plus 28px per all-day row of the anchor
    // week's busiest day, in Day and Week alike, so switching between them
    // never moves the hour axis. Four rows at most; more fold into "+N all day".
    // Month has no all-day band: its weekday row is the 36px alone.
    function previewAllDayAt(index) { return previewRows.filter(function(e) { return e.all_day && e.day_index <= index && e.day_index + e.span_days > index }) }
    readonly property int shownAllDayRows: {
        let most = 0
        if (timeGrid) for (let index = 0; index < dayCount; ++index) most = Math.max(most, allDayAt(index).length + previewAllDayAt(index).length)
        return most
    }
    readonly property int bandRows: Math.min(4, Math.max(backend.calendarBandRows || 0, shownAllDayRows))
    readonly property real bandHeight: 36 + 28 * bandRows
    function bandChips(events, reserved) { const rows = Math.max(1, 4 - reserved); return events.length > rows ? events.slice(0, rows - 1) : events }
    // Hovering an empty slot previews the half hour a click would create.
    property var hoverSlot: ({})
    readonly property bool canCreate: backend.connected && backend.calendarSources.some(function(s) { return s.writable })
    readonly property real ghostHeight: Math.max(hourHeight / 2, 20)
    function hoverAt(index, y) {
        const t = Math.max(0, Math.min(23.5, Math.floor(y / hourHeight * 2) / 2))
        if (hoverSlot.index !== index || hoverSlot.t !== t) hoverSlot = {index: index, t: t}
    }
    function leaveSlot(index) { if (hoverSlot.index === index) hoverSlot = ({}) }
    function slotFree(index, t) {
        const end = t + ghostHeight / hourHeight
        return !eventsAt(index).some(function(e) { return !e.all_day && !calendar.editingEvent(e) && e.start_hour < end && e.start_hour + e.duration_hours > t })
    }
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
    // Saturday and Sunday columns and cells sit in a faint shade.
    function weekend(date) { return date.getDay() % 6 === 0 }
    readonly property color weekendShade: Theme.shadowAlpha(.22)
    function allDayAt(index) { return eventsAt(index).filter(function(e) { return e.all_day && !calendar.editingEvent(e) }) }
    function openDay(index) {
        if (backend.openCalendarDay !== undefined) backend.openCalendarDay(isoAt(index))
        else { backend.setCalendarDate(isoAt(index)); backend.setCalendarView("day") }
    }
    function showEvent(event, sourceItem) { if (!backend.calendarReady || !backend.connected) return; if (editor.opened) { editor.contentItem.forceActiveFocus(); return } details.openAt(event, sourceItem || calendar) }
    // Mail's "Next" row: Day view on the event's day (else today), then its
    // card beside its block, as if clicked. The window may load later, so the
    // request waits for it; navigating elsewhere, leaving Calendar or a
    // request older than 20 s (e.g. Calendar never opened) drops it.
    property var dayEventRequest: null
    function showEventInDay(event) {
        if (!event || !event.id) return
        // Core rows (an invitation email's event) carry no layout date.
        const date = event.date || (backend.calendarDayOf !== undefined ? backend.calendarDayOf(event.start || "") : "") || backend.todayDate
        cancelRange(); details.close(); allDayOverflow.close()
        dayEventRequest = {event: event, date: date, at: Date.now()}
        if (backend.openCalendarDay !== undefined) backend.openCalendarDay(date)
        else { backend.setCalendarDate(date); backend.setCalendarView("day") }
        Qt.callLater(revealDayEvent)
    }
    readonly property bool dayEventReady: !!dayEventRequest && visible && backend.calendarReady
    onDayEventReadyChanged: if (dayEventReady) Qt.callLater(revealDayEvent)
    function eventBlock(root, id) {
        for (const child of root.children) {
            if (!child.visible) continue
            if (child.modelData && child.modelData.id === id && child.width > 0 && /^calendar(Event|AllDay)-/.test(child.objectName)) return child
            const found = eventBlock(child, id)
            if (found) return found
        }
        return null
    }
    function revealDayEvent() {
        const request = dayEventRequest
        if (!request || !visible || !backend.calendarReady || !backend.connected) return
        dayEventRequest = null
        if (Date.now() - request.at > 20000 || backend.calendarView !== "day" || backend.calendarDate !== request.date) return
        const rows = backend.events.filter(function(e) { return e.id === request.event.id })
        const match = rows.find(function(e) { return e.date === request.date }) || rows[0]
        if (!match) return
        if (!match.all_day) {
            const top = match.start_hour * hourHeight, bottom = top + Math.max(20, match.duration_hours * hourHeight)
            if (top < gridScroll.contentY || bottom > gridScroll.contentY + gridScroll.height) {
                retainedTopHour = Math.max(0, match.start_hour - 1)
                preserveGridPosition()
            }
        }
        const block = eventBlock(match.all_day ? dayHeaderRow : grid, match.id)
        if (block) block.forceActiveFocus()
        showEvent(match, block)
    }
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
    // The account colour marks an event; other accounts' events use it at half strength.
    function sourceRule(event) { return event.busy_only ? Theme.inkAlpha(.45) : event.account === backend.account ? Theme.accountColor(event.account) : Theme.accountTint(event.account, .5) }
    readonly property string accountName: (backend.accounts.find(function(a) { return a.id === backend.account }) || {}).name || backend.account
    // Day: the full date, after Today/Tomorrow/Yesterday when one applies.
    function dayTitle() {
        const offset = Math.round((new Date(backend.calendarDate + "T12:00:00") - new Date(backend.todayDate + "T12:00:00")) / 86400000)
        const relative = offset === 0 ? "Today" : offset === 1 ? "Tomorrow" : offset === -1 ? "Yesterday" : ""
        return (relative ? relative + " · " : "") + Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "dddd MMMM d yyyy")
    }
    // Week: the range with the year on both ends. The event count and account are shown elsewhere.
    function subtitle() { return backend.calendarView === "week" ? Qt.formatDate(dateAt(0), "d MMMM yyyy") + " – " + Qt.formatDate(dateAt(6), "d MMMM yyyy") : "" }
    function conferenceLabel(url) {
        const value = String(url || "").toLowerCase()
        return !value ? "" : value.indexOf("meet.google") >= 0 ? "Google Meet" : value.indexOf("teams") >= 0 ? "Teams" : value.indexOf("zoom") >= 0 ? "Zoom" : "Video call"
    }
    function people(event) { return (event.attendees || []).filter(function(a) { return !a.self }).map(function(a) { return a.name || a.email }).join(", ") }
    // Day-view block and list details: other account, place, call and guests.
    function eventSub(event) { return [Theme.responseLabel(event), event.location || conferenceLabel(event.conference_url) || (event.account !== backend.account ? event.account_name || "" : "")].filter(function(s) { return !!s }).join(" · ") }
    function agendaSub(event) { return [Theme.responseLabel(event), event.account !== backend.account ? event.account_name : "", event.location, conferenceLabel(event.conference_url), people(event)].filter(function(s) { return !!s }).join(" · ") }
    function eventPast(event) { return event.date < backend.todayDate || (event.date === backend.todayDate && !event.all_day && event.start_hour + event.duration_hours <= backend.nowHour) }
    function eventNow(event) { return event.date === backend.todayDate && !event.all_day && event.start_hour <= backend.nowHour && event.start_hour + event.duration_hours > backend.nowHour }
    // Showing every account clears a narrower calendar choice; otherwise only
    // the visibility changes, which filters what was read without a reload.
    function setCombined(on) {
        cancelRange(); details.close()
        const patch = {calendar_account_visibility: on ? "all" : "current"}
        if (on && backend.preferences.selected_calendars != null) patch.selected_calendars = null
        backend.setPreferences(patch)
    }
    readonly property bool showDeclined: backend.preferences.calendar_show_declined === true
    function setShowDeclined(on) { details.close(); backend.setPreferences({calendar_show_declined: on}) }
    function preserveGridPosition() {
        if (gridScroll.moving || gridScroll.height <= 0) return
        restoringGridPosition = true
        gridScroll.contentY = Math.max(0, Math.min(retainedTopHour * hourHeight, gridScroll.contentHeight - gridScroll.height))
        laidOutHourHeight = hourHeight
        laidOutGridHeight = gridScroll.height
        restoringGridPosition = false
    }
    // Bring the now line into view when a day or week containing today opens
    // and the remembered position would leave it off screen.
    readonly property string gridRange: timeGrid ? backend.calendarView + ":" + backend.calendarFrom + ":" + backend.todayDate : ""
    onGridRangeChanged: Qt.callLater(revealNow)
    function revealNow() {
        if (!visible || !timeGrid || gridScroll.moving || gridScroll.height <= 0) return
        let today = false
        for (let i = 0; i < dayCount; ++i) today = today || isoAt(i) === backend.todayDate
        if (!today) return
        const y = backend.nowHour * hourHeight
        if (y >= gridScroll.contentY + 12 && y <= gridScroll.contentY + gridScroll.height - 24) return
        retainedTopHour = Math.max(0, backend.nowHour - gridScroll.height / 4 / hourHeight)
        preserveGridPosition()
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
        function onEventsChanged() { if (calendar.rangeDrag.index !== undefined && (calendar.rangeDrag.from !== backend.calendarFrom || calendar.rangeDrag.view !== backend.calendarView)) calendar.cancelRange(); calendar.hoverSlot = ({}); calendar.updateMailActivityContext() }
    }
    onHourHeightChanged: Qt.callLater(preserveGridPosition)
    onVisibleChanged: { updateMailActivityContext(); if (visible) Qt.callLater(revealNow); if (!visible) { dayEventRequest = null; cancelRange(); details.close(); datePicker.close(); moreMenu.close(); allDayOverflow.close() } }
    Shortcut { sequence: ShortcutRegistry.sequence("day",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("day") }
    Shortcut { sequence: ShortcutRegistry.sequence("week",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("week") }
    Shortcut { sequence: ShortcutRegistry.sequence("month",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("month") }
    Shortcut { sequence: ShortcutRegistry.sequence("agenda",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("agenda") }
    Shortcut { sequence: ShortcutRegistry.sequence("newEv",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: calendar.createEvent() }
    Shortcut { sequence: ShortcutRegistry.sequence("back",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.moveCalendar(-1) }
    Shortcut { sequence: ShortcutRegistry.sequence("fwd",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.moveCalendar(1) }
    Shortcut { sequence: ShortcutRegistry.sequence("today",backend.preferences); enabled: calendar.shortcutsEnabled; onActivated: backend.showCalendarToday() }

    // One header-band row: Week fills its column with the title; Day sizes the
    // chip to "All day · Title".
    component AllDayChip: Rectangle {
        id: chip
        required property var modelData
        property bool labelled: false
        property real maximumWidth: 0
        objectName: "calendarAllDay-" + (modelData.event_id || modelData.id)
        width: labelled ? Math.min(chipText.implicitWidth + 22, maximumWidth) : maximumWidth; height: 22; radius: 5
        color: chipMouse.containsMouse ? Theme.hover : Theme.selected
        activeFocusOnTab: visible && backend.calendarReady
        Keys.onReturnPressed: calendar.showEvent(modelData, chip)
        Keys.onSpacePressed: calendar.showEvent(modelData, chip)
        border.width: activeFocus ? 1 : 0; border.color: Theme.accent
        Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
        Accessible.onPressAction: calendar.showEvent(modelData, chip)
        Rectangle { width: 3; height: parent.height; color: parent.color }
        Rectangle { width: 2; height: parent.height; color: calendar.sourceRule(chip.modelData) }
        Text { id: chipText; x: chip.labelled ? 12 : 10; width: parent.width - x - (chip.labelled ? 10 : 8); height: parent.height; text: (chip.labelled ? "All day · " : "") + chip.modelData.title; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; textFormat: Text.PlainText }
        MouseArea { id: chipMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: calendar.showEvent(chip.modelData, chip) }
        FmToolTip { shown: chipMouse.containsMouse && !chip.labelled; text: "All day · " + chip.modelData.title }
    }

    ColumnLayout {
        id: calendarColumn
        anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 0
        // A 40px header row, like Mail's, so switching views keeps the edges still.
        RowLayout {
            objectName: "calendarHeaderRow"
            Layout.fillWidth: true; Layout.preferredHeight: 40; Layout.bottomMargin: 8; spacing: 12
            Item {
                Layout.fillWidth: true; implicitHeight: calendarTitle.implicitHeight
                Text { id: calendarTitle; objectName: "calendarTitle"; anchors.verticalCenter: parent.verticalCenter; text: backend.calendarView === "week" ? "Week " + calendar.weekNumber() : backend.calendarView === "agenda" ? "Agenda" : backend.calendarView === "day" ? calendar.dayTitle() : Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "MMMM yyyy"); color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(15); font.weight: Font.Medium; font.letterSpacing: 0}
                Text { objectName: "calendarSubtitle"; x: calendarTitle.width + 12; width: Math.max(0, parent.width - x); anchors.baseline: calendarTitle.baseline; text: calendar.subtitle(); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); visible: text.length > 0 && calendar.width > 1050; elide: Text.ElideRight; textFormat: Text.PlainText }
            }
            Item {
                implicitWidth: combinedRow.width + 8; implicitHeight: 30
                Row {
                    id: combinedRow; x: 4; anchors.verticalCenter: parent.verticalCenter; spacing: 9
                    FmSwitch {
                        objectName: "calendarCombinedToggle"; checked: calendar.combinedAccounts
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: calendar.setCombined(checked)
                        Accessible.name: "All calendars"
                    }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: "All calendars"; color: calendar.combinedAccounts || combinedHover.hovered ? Theme.text : Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                }
                HoverHandler { id: combinedHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: calendar.setCombined(!calendar.combinedAccounts) }
                FmToolTip { shown: combinedHover.hovered; text: calendar.combinedAccounts ? "Showing every account. Click for " + calendar.accountName + " only." : "Showing " + calendar.accountName + " only. Click to combine all accounts." }
            }
            Item {
                implicitWidth: declinedRow.width + 8; implicitHeight: 30
                Row {
                    id: declinedRow; x: 4; anchors.verticalCenter: parent.verticalCenter; spacing: 9
                    FmSwitch {
                        objectName: "calendarDeclinedToggle"; checked: calendar.showDeclined
                        anchors.verticalCenter: parent.verticalCenter
                        onClicked: calendar.setShowDeclined(checked)
                        Accessible.name: "Show declined"
                    }
                    Text { anchors.verticalCenter: parent.verticalCenter; text: "Show declined"; color: calendar.showDeclined || declinedHover.hovered ? Theme.text : Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                }
                HoverHandler { id: declinedHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: calendar.setShowDeclined(!calendar.showDeclined) }
                FmToolTip { shown: declinedHover.hovered; text: calendar.showDeclined ? "Hide events you declined" : "Show events you declined" }
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
                    FmButton { objectName: "calendarPrevious"; iconName: "previous"; iconSize: 12; width: 30; height: 28; radius: 5; foreground: Theme.inkAlpha(.7); tip: "Previous — Left"; background: Rectangle { radius: 5; color: parent.hovered ? Theme.surface : "transparent"; border.width: parent.visualFocus ? 1 : 0; border.color: Theme.focusBorder } onClicked: backend.moveCalendar(-1) }
                    Rectangle { width: Theme.ruleWidth; height: 28; color: Theme.controlBorder }
                    FmButton {
                        id: dateButton; objectName: "calendarDatePickerButton"; height: 28; leftPadding: 12; rightPadding: 12
                        width: Math.max(112, implicitWidth); font.pointSize: Theme.fontPoints(12); font.weight: Font.Normal
                        iconName: "down"; iconSize: 10; iconSpacing: 8; iconTrailing: true; iconColor: Theme.inkAlpha(.4)
                        foreground: backend.calendarDate === backend.todayDate ? Theme.text : Theme.inkAlpha(.85)
                        text: backend.calendarView === "week" ? Qt.formatDate(calendar.dateAt(0), calendar.dateAt(0).getMonth() === calendar.dateAt(6).getMonth() ? "d" : "d MMM") + " – " + Qt.formatDate(calendar.dateAt(6), "d MMM") : backend.calendarView === "month" ? Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "MMM yyyy") : backend.calendarDate === backend.todayDate ? "Today" : Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "ddd d MMM")
                        selected: datePicker.opened; tip: "Pick a date — T for today"
                        background: Rectangle { color: datePicker.opened || parent.hovered ? Theme.surface : "transparent"; border.width: parent.visualFocus ? 1 : 0; border.color: Theme.focusBorder }
                        onClicked: { datePicker.month = new Date(backend.calendarDate + "T12:00:00"); datePicker.opened ? datePicker.close() : datePicker.open() }
                    }
                    Rectangle { width: Theme.ruleWidth; height: 28; color: Theme.controlBorder }
                    FmButton { objectName: "calendarNext"; iconName: "next"; iconSize: 12; width: 30; height: 28; radius: 5; foreground: Theme.inkAlpha(.7); tip: "Next — Right"; background: Rectangle { radius: 5; color: parent.hovered ? Theme.surface : "transparent"; border.width: parent.visualFocus ? 1 : 0; border.color: Theme.focusBorder } onClicked: backend.moveCalendar(1) }
                }
            }
            FmButton { id: moreButton; objectName: "calendarMoreButton"; iconName: "more"; implicitWidth: 28; implicitHeight: 28; tip: "Calendar options and coverage"; onClicked: moreMenu.open() }
        }
        Text { visible: !backend.calendarReady && (!calendar.calendarLoading || calendar.slowLoad); text: backend.calendarCoverage; Layout.fillWidth: true; Layout.bottomMargin: 8; font.pointSize: Theme.fontPoints(10.5); color: Theme.subtle; wrapMode: Text.Wrap }
        // Header band. A Week column shows its date, mail count and all-day
        // chips; Day view shows only its chips, resting on the rule, since the
        // title already names the day.
        RowLayout {
            id: dayHeaderRow; objectName: "calendarBand"
            visible: calendar.timeGrid
            Layout.fillWidth: true; Layout.fillHeight: false
            Layout.preferredHeight: calendar.bandHeight; Layout.maximumHeight: calendar.bandHeight
            Layout.leftMargin: 56; Layout.rightMargin: backend.calendarView === "day" && calendar.width > 850 ? 352 : 0; spacing: 0
            Repeater { model: calendar.timeGrid ? calendar.dayCount : 0
                Item {
                    id: dayHeader
                    required property int index
                    readonly property string iso: calendar.isoAt(index)
                    readonly property bool today: iso === backend.todayDate
                    readonly property bool single: calendar.dayCount === 1
                    readonly property var allDay: calendar.allDayAt(index)
                    readonly property var previews: calendar.previewAllDayAt(index)
                    readonly property var chips: calendar.bandChips(allDay, previews.length)
                    objectName: "calendarDayHeader-" + index
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1
                    clip: true
                    MouseArea {
                        id: headerOpen
                        anchors.fill: parent; enabled: !dayHeader.single; hoverEnabled: true; cursorShape: Qt.PointingHandCursor
                        onClicked: calendar.openDay(dayHeader.index)
                    }
                    FmToolTip { shown: headerOpen.containsMouse; text: "Open day" }
                    Item {
                        visible: !dayHeader.single
                        x: 10; width: parent.width - 16; height: 26
                        Text { id: dayNumber; anchors.verticalCenter: parent.verticalCenter; text: Qt.formatDate(calendar.dateAt(dayHeader.index), "d"); color: dayHeader.today ? Theme.accent : dayHeader.iso < backend.todayDate ? Theme.inkAlpha(.5) : calendar.weekend(calendar.dateAt(dayHeader.index)) ? Theme.inkAlpha(.6) : Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(19); font.weight: Font.Medium; font.letterSpacing: 0}
                        Text { x: dayNumber.width + 8; anchors.verticalCenter: parent.verticalCenter; text: Qt.formatDate(calendar.dateAt(dayHeader.index), "ddd"); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                        CalendarMailCount {
                            objectName: "calendarMailCount-" + dayHeader.iso
                            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                            date: dayHeader.iso; row: calendar.mailDays[date]
                            accountName: calendar.accountName
                            compact: dayHeader.width < 165
                            onClicked: calendar.openMailRequested()
                        }
                    }
                    Column {
                        x: 10; width: parent.width - 16; spacing: 6
                        y: dayHeader.single ? parent.height - 10 - height : 32
                        Repeater { model: dayHeader.chips; AllDayChip { labelled: dayHeader.single; maximumWidth: parent.width } }
                        Repeater {
                            model: dayHeader.previews
                            Rectangle {
                                required property var modelData; objectName: "calendarAllDayPreview-" + dayHeader.index
                                width: dayHeader.single ? Math.min(previewText.implicitWidth + 22, parent.width) : parent.width; height: 22
                                radius: 5; color: Theme.accentAlpha(0.125); border.color: Theme.accent
                                Text { id: previewText; x: 10; width: parent.width - 18; height: parent.height; text: (dayHeader.single ? "All day · " : "") + parent.modelData.title; color: Theme.accent; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; textFormat: Text.PlainText }
                            }
                        }
                        FmButton {
                            objectName: "calendarAllDayMore-" + dayHeader.index
                            visible: dayHeader.allDay.length > dayHeader.chips.length
                            text: "+ " + (dayHeader.allDay.length - dayHeader.chips.length) + " all day"
                            implicitHeight: 22; leftPadding: 4; rightPadding: 4; font.pointSize: Theme.fontPoints(10.5)
                            onClicked: { allDayOverflow.eventRows = dayHeader.allDay; allDayOverflow.open() }
                        }
                    }
                    // Only Week view underlines today, in accent.
                    Rectangle { y: parent.height - height; width: parent.width; height: Theme.ruleWidth; color: dayHeader.today && !dayHeader.single ? Theme.accent : Theme.line }
                }
            }
        }
        RowLayout {
            id: gridRow
            visible: backend.calendarView === "day" || backend.calendarView === "week"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; spacing: 32
            Flickable {
                id: gridScroll; objectName: "calendarGrid"; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: width; contentHeight: 24 * calendar.hourHeight + 10; boundsBehavior: Flickable.StopAtBounds
                Component.onCompleted: { Qt.callLater(calendar.preserveGridPosition); Qt.callLater(calendar.revealNow) }
                onHeightChanged: Qt.callLater(calendar.preserveGridPosition)
                onContentYChanged: calendar.rememberGridPosition()
                onMovementEnded: { calendar.retainedTopHour = Math.max(0, contentY) / calendar.hourHeight; backend.remember("calendarY", contentY); Qt.callLater(calendar.preserveGridPosition) }
                ScrollBar.vertical: FmScrollBar {}
                Repeater { model: 25
                    Text { required property int index; objectName: "calendarHour-" + index; x: 0; y: Math.max(0, index * calendar.hourHeight - (index === 24 ? height : index === 0 ? 0 : height / 2)); text: (index < 10 ? "0" : "") + index + ":00"; color: Theme.inkAlpha(index >= calendar.workingStart && index <= calendar.workingEnd ? .5 : .3); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.features: {"tnum": 1} }
                }
                Item {
                    id: grid; x: 56; width: gridScroll.width - 56; height: 24 * calendar.hourHeight; property real dayWidth: width / calendar.dayCount
                    Repeater { model: calendar.timeGrid ? calendar.dayCount : 0
                        Rectangle {
                            required property int index; x: index * grid.dayWidth; width: grid.dayWidth; height: grid.height
                            color: calendar.isoAt(index) === backend.todayDate ? Theme.bg1 : calendar.dayCount === 7 && calendar.weekend(calendar.dateAt(index)) ? calendar.weekendShade : "transparent"
                            Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: Theme.ruleWidth; height: parent.height; color: Theme.line }
                            MouseArea {
                                objectName: "calendarSlot-" + parent.index
                                anchors.fill: parent; cursorShape: Qt.CrossCursor; preventStealing: calendar.rangeDrag.index !== undefined
                                hoverEnabled: true
                                onPressed: function(mouse) { if (!calendar.beginRange(parent.index, mouse.y)) mouse.accepted = false }
                                onPositionChanged: function(mouse) { if (pressed) calendar.moveRange(mouse.y); else calendar.hoverAt(parent.index, mouse.y) }
                                onExited: calendar.leaveSlot(parent.index)
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
                    // Decoration only: clicks and drags on the column create events.
                    Item {
                        id: slotGhost; objectName: "calendarSlotGhost"
                        readonly property var slot: calendar.hoverSlot
                        visible: slot.index !== undefined && slot.index < calendar.dayCount && calendar.canCreate && calendar.rangeDrag.index === undefined
                            && !editor.opened && calendar.slotFree(slot.index, slot.t)
                        x: (slot.index || 0) * grid.dayWidth + 4; width: Math.max(0, grid.dayWidth - 10)
                        height: calendar.ghostHeight; y: Math.min((slot.t || 0) * calendar.hourHeight, 24 * calendar.hourHeight - height)
                        Rectangle { anchors.fill: parent; radius: 5; color: Theme.inkAlpha(.035) }
                        Shape {
                            anchors.fill: parent
                            ShapePath {
                                strokeColor: Theme.inkAlpha(.22); strokeWidth: 1; strokeStyle: ShapePath.DashLine; dashPattern: [3, 3]; fillColor: "transparent"
                                PathSvg { path: "M 2 .5 H " + (slotGhost.width - 4) + " A 3.5 3.5 0 0 1 " + (slotGhost.width - .5) + " 4 V " + (slotGhost.height - 4) + " A 3.5 3.5 0 0 1 " + (slotGhost.width - 4) + " " + (slotGhost.height - .5) + " H 2" }
                            }
                        }
                        Rectangle { width: 2; height: parent.height; color: Theme.inkAlpha(.35) }
                        Row {
                            x: 9; width: parent.width - 16; anchors.verticalCenter: parent.verticalCenter; spacing: 5; clip: true
                            FmIcon { name: "plus"; size: 9; color: Theme.inkAlpha(.55); anchors.verticalCenter: parent.verticalCenter }
                            Text { text: calendar.hourText(slotGhost.slot.t || 0) + " – " + calendar.hourText((slotGhost.slot.t || 0) + .5); color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.features: {"tnum": 1}; anchors.verticalCenter: parent.verticalCenter }
                        }
                    }
                    Rectangle {
                        objectName: "calendarRangePreview"
                        readonly property real low: Math.min(calendar.rangeDrag.start || 0, calendar.rangeDrag.end || 0)
                        readonly property real high: Math.max(calendar.rangeDrag.start || 0, calendar.rangeDrag.end || 0)
                        visible: calendar.rangeDrag.index !== undefined
                        x: (calendar.rangeDrag.index || 0) * grid.dayWidth + 4; y: low * calendar.hourHeight
                        width: grid.dayWidth - 10; height: Math.max(calendar.hourHeight / 2, (high - low) * calendar.hourHeight)
                        color: Theme.accentAlpha(.1); radius: 5
                        // Dashes exist only while a range is being dragged.
                        Repeater { model: parent.visible ? Math.max(0, Math.ceil(parent.width / 8)) : 0; Rectangle { required property int index; x: Theme.snapX(parent, index * 8); y: Theme.snapY(parent, 0); width: Math.max(0, Math.min(4, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                        Repeater { model: parent.visible ? Math.max(0, Math.ceil(parent.width / 8)) : 0; Rectangle { required property int index; x: Theme.snapX(parent, index * 8); y: Theme.snapY(parent, parent.height - 1); width: Math.max(0, Math.min(4, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                        Rectangle { width: 2; height: parent.height; color: Theme.accent }
                        Text { anchors.fill: parent; anchors.margins: 4; text: calendar.hourText(parent.low) + " – " + calendar.hourText(parent.high); color: Theme.accent; font.pointSize: Theme.fontPoints(10.5); elide: Text.ElideRight }
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
                            // Invitations: accepted as usual, Maybe striped, not answered
                            // outlined, declined (shown on request) faint and struck through.
                            readonly property var rsvp: Theme.rsvpOf(modelData)
                            readonly property bool maybe: rsvp === "maybe"
                            readonly property bool unanswered: rsvp === ""
                            readonly property bool declined: rsvp === "no"
                            readonly property bool hollow: unanswered || declined
                            radius: calendar.dayCount === 1 ? 4 : 3; color: hollow ? (eventMouse.containsMouse ? Theme.inkAlpha(.04) : "transparent") : eventMouse.containsMouse ? Theme.hover : Theme.selected; clip: true
                            MaybeBands { anchors.fill: parent; visible: eventBox.maybe }
                            DashedOutline { anchors.fill: parent; visible: eventBox.unanswered && !eventBox.activeFocus; radius: eventBox.radius; color: Theme.accountTint(eventBox.modelData.account, .45) }
                            Rectangle { width: 4; height: parent.height; color: parent.color; visible: !eventBox.hollow }
                            Rectangle { width: 2; height: parent.height; color: eventBox.declined ? Theme.inkAlpha(.18) : eventBox.maybe ? Theme.accountTint(eventBox.modelData.account, .6) : eventBox.unanswered ? Theme.accountTint(eventBox.modelData.account, .45) : calendar.sourceRule(eventBox.modelData) }
                            Row {
                                anchors.top: parent.top; anchors.topMargin: 3; anchors.left: parent.left; anchors.leftMargin: calendar.dayCount === 1 ? 14 : 9; anchors.right: parent.right; anchors.rightMargin: accountBadge.visible ? 22 : 7; spacing: calendar.dayCount === 1 ? 14 : 6
                                Text { id: eventTime; visible: eventBox.width >= 64 || calendar.dayCount === 1; y: eventTitle.baselineOffset - baselineOffset; text: eventBox.modelData.time_display + (calendar.dayCount === 1 ? " – " + eventBox.modelData.end_display : ""); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(calendar.dayCount === 1 ? 11 : 10.5); font.features: {"tnum": 1} }
                                Text { id: eventTitle; text: eventBox.modelData.title; textFormat: Text.PlainText; color: eventBox.declined ? Theme.inkAlpha(.35) : eventBox.unanswered ? Theme.inkAlpha(.7) : eventBox.maybe ? Theme.inkAlpha(.85) : Theme.text; font.strikeout: eventBox.declined; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(calendar.dayCount === 1 ? 12 : 11.5); font.weight: Font.Medium; width: Math.min(implicitWidth, Math.max(0, parent.width - (eventTime.visible ? eventTime.width + parent.spacing : 0))); elide: Text.ElideRight }
                                Text { visible: calendar.dayCount === 1 && text.length > 0; y: eventTitle.baselineOffset - baselineOffset; text: calendar.eventSub(eventBox.modelData); width: Math.max(0, parent.width - eventTime.width - eventTitle.width - 2 * parent.spacing); color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); elide: Text.ElideRight; textFormat: Text.PlainText }
                            }
                            MouseArea { id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { eventBox.forceActiveFocus(); calendar.showEvent(eventBox.modelData, eventBox) } }
                            FmToolTip { shown: eventMouse.containsMouse; text: eventBox.modelData.title + " · " + eventBox.modelData.start_display + (Theme.responseLabel(eventBox.modelData) ? " · " + Theme.responseLabel(eventBox.modelData) : "") + "\n" + eventBox.modelData.account_name + " / " + eventBox.modelData.calendar_name }
                            AccountBadge {
                                id: accountBadge; diameter: 14
                                anchors.top: parent.top; anchors.right: parent.right; anchors.topMargin: 3; anchors.rightMargin: 4
                                account: eventBox.modelData.account || ""
                                accountName: eventBox.modelData.account_name || ""
                                selected: true
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
                            height: Math.max(20, modelData.duration_hours * calendar.hourHeight); color: Theme.accentAlpha(.125); radius: 5
                            Repeater { model: Math.max(0, Math.ceil(parent.width / 10)); Rectangle { required property int index; x: Theme.snapX(parent, index * 10); y: Theme.snapY(parent, 0); width: Math.max(0, Math.min(5, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                            Repeater { model: Math.max(0, Math.ceil(parent.width / 10)); Rectangle { required property int index; x: Theme.snapX(parent, index * 10); y: Theme.snapY(parent, parent.height - 1); width: Math.max(0, Math.min(5, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                            Rectangle { width: 2; height: parent.height; color: Theme.accent }
                            Text { anchors.fill: parent; anchors.margins: 7; text: editor.preview.title; color: Theme.accent; font.pointSize: Theme.fontPoints(11.5); elide: Text.ElideRight; textFormat: Text.PlainText }
                        }
                    }
                }
            }
            Item { visible: backend.calendarView === "day" && calendar.width > 850; Layout.preferredWidth: 320; Layout.fillHeight: true }
        }
        // Month: a 36px weekday header and a 56px week axis where the hour
        // axis stands. Cells draw only right and
        // bottom rules; the axis rule and header rule close the grid.
        RowLayout {
            objectName: "calendarMonthBand"
            visible: calendar.monthView; Layout.fillWidth: true; Layout.fillHeight: false; spacing: 0
            Item { Layout.preferredWidth: 56; implicitHeight: 36 }
            Repeater { model: 7
                Item {
                    required property int index
                    Layout.fillWidth: true; Layout.preferredWidth: 1; implicitHeight: 36
                    Text {
                        objectName: "calendarMonthWeekday-" + parent.index
                        x: 10; height: 26; verticalAlignment: Text.AlignVCenter
                        text: Qt.formatDate(calendar.dateAt(parent.index), "ddd")
                        color: calendar.weekend(calendar.dateAt(parent.index)) ? Theme.inkAlpha(.3) : Theme.inkAlpha(.5)
                        font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
                    }
                    Rectangle { y: parent.height - height; width: parent.width; height: Theme.ruleWidth; color: Theme.line }
                }
            }
        }
        RowLayout {
            visible: calendar.monthView; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; spacing: 0
            Item {
                id: weekAxis
                readonly property int rows: Math.max(1, Math.ceil(calendar.monthCells / 7))
                Layout.preferredWidth: 56; Layout.fillHeight: true
                Repeater { model: weekAxis.rows
                    Item {
                        id: weekRow
                        required property int index
                        readonly property var start: calendar.monthDates[index * 7] || calendar.emptyDay
                        readonly property int number: start.iso ? calendar.isoWeek((calendar.monthDates[index * 7 + 3] || start).date) : 0
                        readonly property bool current: !!start.iso && backend.todayDate >= start.iso && backend.todayDate <= (calendar.monthDates[index * 7 + 6] || start).iso
                        // The current week opens on today; any other on its first day.
                        function open() { if (start.iso) calendar.openWeek(current ? backend.todayDate : start.iso) }
                        objectName: "calendarWeekNumber-" + index
                        y: index * weekAxis.height / weekAxis.rows; width: weekAxis.width - 1; height: weekAxis.height / weekAxis.rows
                        Accessible.role: Accessible.Button; Accessible.name: "Open week " + number
                        Accessible.onPressAction: open()
                        Text { y: 9; text: weekRow.number ? "W" + weekRow.number : ""; color: weekMouse.containsMouse ? Theme.text : Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.features: {"tnum": 1} }
                        MouseArea { id: weekMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: weekRow.open() }
                        FmToolTip { shown: weekMouse.containsMouse; text: "Open week " + weekRow.number }
                    }
                }
                Rectangle { x: parent.width - width; width: Theme.ruleWidth; height: parent.height; color: Theme.line }
            }
            GridLayout {
                id: monthGrid
                Layout.fillWidth: true; Layout.fillHeight: true; columns: 7; columnSpacing: 0; rowSpacing: 0
                Repeater { model: 42
                    Rectangle {
                        id: monthCell; objectName: "calendarMonthCell-" + day.iso; required property int index; visible: index < calendar.monthCells
                        readonly property var day: calendar.monthDates[index] || calendar.emptyDay
                        property var dayEvents: calendar.monthEvents[index] || []; property bool inMonth: day.date.getMonth() === calendar.monthNumber
                        readonly property bool isToday: day.iso === backend.todayDate
                        readonly property bool past: !!day.iso && day.iso < backend.todayDate
                        readonly property bool weekendDay: !!day.iso && calendar.weekend(day.date)
                        readonly property int fit: Math.min(3, Math.max(0, Math.floor((height - 50) / 18) - (dayEvents.length * 18 > height - 50 ? 1 : 0)))
                        // Today orders finished timed events first, then all-day and upcoming
                        // ones, so the now line sits where the day is: at the top just after
                        // midnight. It shows a window around now: the last finished event,
                        // then the rest; two rows when the now line is needed and more wait.
                        readonly property var finishedEvents: isToday ? dayEvents.filter(function(e) { return !e.all_day && e.start_hour + e.duration_hours <= backend.nowHour }) : []
                        readonly property var rows: isToday ? finishedEvents.concat(dayEvents.filter(function(e) { return finishedEvents.indexOf(e) < 0 })) : dayEvents
                        readonly property int upcoming: isToday && finishedEvents.length < rows.length ? finishedEvents.length : -1
                        readonly property bool needLine: isToday && rows.length > 0
                        readonly property int cap: Math.min(fit, needLine && rows.length > 2 ? 2 : 3)
                        readonly property int first: !isToday || upcoming === 0 ? 0 : upcoming > 0 ? Math.max(0, Math.min(upcoming - 1, rows.length - cap)) : Math.max(0, rows.length - cap)
                        readonly property int shown: Math.max(0, Math.min(cap, rows.length - first))
                        readonly property int nowRow: needLine ? (upcoming >= first && upcoming < first + shown ? upcoming - first : -1) : -2
                        function eventDone(e) { return isToday ? !e.all_day && e.start_hour + e.duration_hours <= backend.nowHour : past }
                        Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1; Layout.preferredHeight: 1; clip: true
                        color: monthHover.hovered ? Theme.bg1 : isToday ? Theme.accentAlpha(.06) : weekendDay ? calendar.weekendShade : "transparent"
                        Rectangle { x: parent.width - width; width: Theme.ruleWidth; height: parent.height; color: Theme.line }
                        Rectangle { y: parent.height - height; width: parent.width; height: Theme.ruleWidth; color: Theme.line }
                        Rectangle { objectName: "calendarMonthToday"; visible: monthCell.isToday; anchors.fill: parent; color: "transparent"; border.width: 1; border.color: Theme.accentAlpha(.45) }
                        HoverHandler { id: monthHover }
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: calendar.openDay(monthCell.index) }
                        FmToolTip { shown: monthHover.hovered && !monthDate.hovered; text: shown ? "Open " + Qt.formatDate(monthCell.day.date, "dddd d MMMM") : "" }
                        Column { x: 8; y: 6; width: parent.width - 16; spacing: 3
                            Item { width: parent.width; height: Math.round(Theme.fontSize(19) * 1.2) + 2
                                FmButton { id: monthDate; objectName: "calendarMonthDate-" + monthCell.index; text: monthCell.day.iso ? monthCell.day.date.getDate() : ""; height: Math.round(Theme.fontSize(19) * 1.2); leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0; textAlignment: Text.AlignLeft; font.pointSize: Theme.fontPoints(19); font.weight: Font.Medium; font.letterSpacing: 0; foreground: monthCell.isToday ? Theme.accent : !monthCell.inMonth ? Theme.inkAlpha(.3) : monthCell.past ? Theme.inkAlpha(.5) : monthCell.weekendDay ? Theme.inkAlpha(.6) : Theme.text; background: Item {} onClicked: calendar.openDay(monthCell.index) }
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
                            Repeater { model: Math.min(3, monthCell.rows.length)
                                Rectangle {
                                    id: monthEvent; required property int index; readonly property var modelData: monthCell.rows[monthCell.first + index] || ({}); visible: index < monthCell.shown; width: monthCell.width - 16; height: Math.round(Theme.fontSize(11) * 1.3); color: "transparent"
                                    readonly property var rsvp: Theme.rsvpOf(modelData)
                                    readonly property real doneOpacity: rsvp === "no" ? .4 : monthCell.eventDone(modelData) ? .5 : 1
                                    ChipEdge {
                                        height: parent.height; opacity: monthEvent.doneOpacity
                                        style: monthEvent.rsvp === "maybe" ? "dashed" : monthEvent.rsvp === "" ? "dotted" : "solid"
                                        color: monthEvent.rsvp === "no" || !monthCell.inMonth ? Theme.line3 : calendar.sourceRule(monthEvent.modelData)
                                    }
                                    // The now line takes no height: drawn over the gap above this row, or
                                    // below the last shown row when every shown event has finished.
                                    Item {
                                        objectName: "calendarNowLine-" + monthCell.day.iso
                                        readonly property bool before: monthEvent.index === monthCell.nowRow
                                        readonly property bool after: monthCell.nowRow === -1 && monthEvent.index === monthCell.shown - 1
                                        visible: before || after; z: 3
                                        x: 4; y: before ? -2 : monthEvent.height + 1; width: monthEvent.width - 4; height: 1
                                        Rectangle { width: parent.width; height: 1; color: Theme.accent }
                                        Rectangle { x: -4; y: -3; width: 7; height: 7; radius: 3.5; color: Theme.accent }
                                    }
                                    FmButton {
                                        objectName:"calendarMonthEvent-"+(parent.modelData.event_id||parent.modelData.id); anchors.fill: parent; opacity: monthEvent.doneOpacity; leftPadding: 8; rightPadding: 0; topPadding: 0; bottomPadding: 0
                                        tip: (monthEvent.modelData.all_day ? "All day" : monthEvent.modelData.time_display + " – " + monthEvent.modelData.end_display) + " · " + monthEvent.modelData.title + (Theme.responseLabel(monthEvent.modelData) ? " · " + Theme.responseLabel(monthEvent.modelData) : "")
                                        Accessible.name: monthEvent.modelData.title || ""
                                        background: Item {}
                                        contentItem: Row {
                                            spacing: 4
                                            Text { id: monthEventTime; visible: !monthEvent.modelData.all_day; text: monthEvent.modelData.time_display || ""; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum": 1}; anchors.verticalCenter: parent.verticalCenter }
                                            Text { text: monthEvent.modelData.title || ""; width: Math.max(0, monthEvent.width - 8 - (monthEventTime.visible ? monthEventTime.width + 4 : 0)); color: !monthCell.inMonth ? Theme.inkAlpha(.45) : Theme.text; font.strikeout: monthEvent.rsvp === "no"; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); elide: Text.ElideRight; textFormat: Text.PlainText; anchors.verticalCenter: parent.verticalCenter }
                                        }
                                        onClicked: calendar.showEvent(monthEvent.modelData, monthEvent)
                                    }
                                }
                            }
                            Loader {
                                active: monthCell.rows.length > monthCell.shown; visible: active
                                sourceComponent: FmButton { text: "+ " + (monthCell.rows.length - monthCell.shown) + " more"; height: Math.round(Theme.fontSize(10.5) * 1.4); leftPadding: 8; rightPadding: 0; topPadding: 0; bottomPadding: 0; textAlignment: Text.AlignLeft; foreground: hovered ? Theme.text : Theme.inkAlpha(.45); font.pointSize: Theme.fontPoints(10.5); background: Item {} onClicked: calendar.openDay(monthCell.index) }
                            }
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
                    Text { text: modelData.start_display; color: Theme.subtle; font.pointSize: Theme.fontPoints(11.5); Layout.preferredWidth: 200 }
                    Column { Layout.fillWidth: true; spacing: 4
                        Text { text: modelData.title; color: Theme.text; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                        Text { text: modelData.account_name + " / " + modelData.calendar_name + (modelData.location ? " · " + modelData.location : ""); color: Theme.subtle; font.pointSize: Theme.fontPoints(10.5); textFormat: Text.PlainText }
                    }
                }
            }
            Text { anchors.centerIn: parent; visible: agenda.count === 0; text: backend.calendarReady ? "Nothing scheduled in this range." : backend.calendarCoverage; color: Theme.subtle; font.pointSize: Theme.fontPoints(12) }
        }
    }
    // Day sidebar: the day's mail in a row as tall as the header band, then
    // its agenda. Without mail an empty row keeps the agenda in place.
    Item {
        id: daySidebar
        visible: backend.calendarView === "day" && calendar.width > 850
        x: calendar.width - 24 - width; y: calendarColumn.y + dayHeaderRow.y; width: 320
        height: Math.max(0, calendarColumn.y + gridRow.y + gridRow.height - y)
        readonly property var mail: calendar.mailDays[backend.calendarDate]
        readonly property bool hasMail: !!mail && backend.calendarDate <= backend.todayDate
        Item {
            id: dayMail; objectName: "calendarDayMail"
            width: parent.width; height: calendar.bandHeight
            activeFocusOnTab: daySidebar.visible && daySidebar.hasMail
            Accessible.role: Accessible.Button
            Accessible.name: daySidebar.hasMail ? daySidebar.mail.received + " received, " + daySidebar.mail.sent + " sent, " + calendar.accountName + ". Open Mail" : ""
            Accessible.onPressAction: if (daySidebar.hasMail) calendar.openMailRequested()
            Keys.onReturnPressed: if (daySidebar.hasMail) calendar.openMailRequested()
            Keys.onSpacePressed: if (daySidebar.hasMail) calendar.openMailRequested()
            Row {
                visible: daySidebar.hasMail
                y: (parent.height - 10 - height) / 2; spacing: 16
                Row {
                    spacing: 6
                    FmIcon { name: "arrowDown"; size: 11; color: Theme.inkAlpha(.5); anchors.verticalCenter: parent.verticalCenter }
                    Text { id: receivedCount; objectName: "calendarDayMailReceived"; text: daySidebar.hasMail ? daySidebar.mail.received : 0; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(17); font.weight: Font.Medium; font.letterSpacing: 0; font.features: {"tnum": 1} }
                    Text { y: receivedCount.baselineOffset - baselineOffset; text: "received"; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                }
                Row {
                    spacing: 6
                    FmIcon { name: "arrowUp"; size: 11; color: Theme.inkAlpha(.5); anchors.verticalCenter: parent.verticalCenter }
                    Text { id: sentCount; objectName: "calendarDayMailSent"; text: daySidebar.hasMail ? daySidebar.mail.sent : 0; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(17); font.weight: Font.Medium; font.letterSpacing: 0; font.features: {"tnum": 1} }
                    Text { y: sentCount.baselineOffset - baselineOffset; text: "sent"; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                }
            }
            FmIcon { visible: daySidebar.hasMail; x: parent.width - width; y: (parent.height - 10 - height) / 2; name: "next"; size: 12; color: dayMailMouse.containsMouse ? Theme.text : Theme.inkAlpha(.35) }
            Rectangle { y: parent.height - height; width: parent.width; height: Theme.ruleWidth; color: dayMail.activeFocus ? Theme.focusBorder : Theme.line }
            MouseArea { id: dayMailMouse; anchors.fill: parent; enabled: daySidebar.hasMail; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: calendar.openMailRequested() }
            FmToolTip { shown: dayMailMouse.containsMouse; text: daySidebar.hasMail ? daySidebar.mail.received + " received · " + daySidebar.mail.sent + " sent · " + calendar.accountName : "" }
        }
        ListView {
            id: dayAgenda; objectName: "calendarDayAgenda"
            y: dayMail.height; width: parent.width
            height: Math.max(0, parent.height - y); clip: true; model: daySidebar.visible ? calendar.agendaEvents : []
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: FmScrollBar {}
            delegate: ItemDelegate {
                id: dayAgendaRow; required property var modelData
                readonly property bool past: calendar.eventPast(modelData)
                width: ListView.view.width; topPadding: 12; bottomPadding: 12; leftPadding: 0; rightPadding: 0
                implicitHeight: dayAgendaText.implicitHeight + 25
                onClicked: calendar.showEvent(modelData, this)
                background: Rectangle { color: dayAgendaRow.hovered ? Theme.surface : "transparent"; border.width: dayAgendaRow.visualFocus ? 1 : 0; border.color: Theme.focusBorder; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
                contentItem: Item {
                    Text { width: 92; text: dayAgendaRow.modelData.all_day ? "All day" : dayAgendaRow.modelData.time_display + " – " + dayAgendaRow.modelData.end_display; color: calendar.eventNow(dayAgendaRow.modelData) ? Theme.accent : Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); font.features: {"tnum": 1}; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12) * 1.4); elide: Text.ElideRight }
                    Column {
                        id: dayAgendaText; x: 106; width: parent.width - x; spacing: 2
                        Text { width: parent.width; text: dayAgendaRow.modelData.title; textFormat: Text.PlainText; color: dayAgendaRow.past || Theme.eventDeclined(dayAgendaRow.modelData) ? Theme.inkAlpha(.45) : Theme.eventOpen(dayAgendaRow.modelData) ? Theme.inkAlpha(.7) : Theme.text; font.strikeout: Theme.eventDeclined(dayAgendaRow.modelData); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); font.weight: Font.Medium; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12.5) * 1.4); elide: Text.ElideRight }
                        Text { width: parent.width; visible: text.length > 0; text: calendar.agendaSub(dayAgendaRow.modelData); textFormat: Text.PlainText; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5); lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(11.5) * 1.4); elide: Text.ElideRight }
                    }
                }
            }
            Text { visible: dayAgenda.count === 0; y: 12; text: "Nothing scheduled."; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
        }
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
                Text { text: Qt.formatDate(datePicker.month, backend.calendarView === "month" ? "yyyy" : "MMMM yyyy"); color: Theme.text; font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter }
                FmButton { iconName: "next"; implicitWidth: 26; implicitHeight: 26; onClicked: datePicker.month = new Date(datePicker.month.getFullYear() + (backend.calendarView === "month" ? 1 : 0), datePicker.month.getMonth() + (backend.calendarView === "month" ? 0 : 1), 1) }
            }
            GridLayout {
                visible: backend.calendarView !== "month"; Layout.fillWidth: true; columns: 7; rowSpacing: 2; columnSpacing: 0
                Repeater { model: 7; Text { required property int index; text: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][(index + datePicker.weekStart) % 7]; color: Theme.subtle; font.pointSize: Theme.fontPoints(10.5); Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter } }
                Repeater { model: datePicker.visible ? datePicker.days : 0
                    FmButton {
                        required property int index; property date day: datePicker.dateFor(index); property string iso: Qt.formatDate(day, "yyyy-MM-dd")
                        text: day.getDate(); Layout.fillWidth: true; Layout.preferredWidth: 1; implicitHeight: 30; leftPadding: 0; rightPadding: 0; font.pointSize: Theme.fontPoints(11.5)
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
            Text { visible: backend.calendarView === "week"; text: "Pick any day — the whole week opens"; color: Theme.subtle; font.pointSize: Theme.fontPoints(10.5); Layout.alignment: Qt.AlignHCenter }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            RowLayout { Layout.fillWidth: true
                FmButton { text: "Today"; foreground: Theme.accent; implicitHeight: 24; font.pointSize: Theme.fontPoints(11.5); onClicked: { backend.showCalendarToday(); datePicker.close() } }
                Text { text: backend.preferences.calendar_display_time_zone || "Time zone unavailable"; color: Theme.subtle; font.pointSize: Theme.fontPoints(10.5); Layout.fillWidth: true; horizontalAlignment: Text.AlignRight }
            }
        }
    }
    Popup {
        id: moreMenu; parent: moreButton; x: parent.width - width; y: 34; width: 255; padding: 8; focus: true
        background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 4
            FmButton { objectName: "calendarAgendaButton"; text: "Agenda"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { backend.setCalendarView("agenda"); moreMenu.close() } }
            FmButton { objectName: "calendarActivityButton"; text: backend.syntheticProfile ? "Local calendar activity" : "Calendar activity"; Layout.fillWidth: true; textAlignment: Text.AlignLeft; onClicked: { moreMenu.close(); operationHistory.open() } }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            Text { text: "Combined · " + (backend.preferences.calendar_display_time_zone || "Time zone unavailable") + "\n" + backend.calendarCoverage; color: Theme.subtle; font.pointSize: Theme.fontPoints(10.5); wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }
    EventDetails { id: details; onEditRequested: function(event) { editor.editEvent = event; editor.openForDate(event.start.slice(0, 10)) } }
    EventEditor { id: editor }
    CalendarOperations { id: operationHistory }
    CalendarEventList { id: allDayOverflow; title: "All-day events"; available: backend.calendarReady && backend.connected; onEventSelected: function(event) { calendar.showEvent(event) } }
}
