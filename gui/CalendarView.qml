import QtQuick
import QtQuick.Window
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: calendar
    objectName: "calendarWorkspace"
    property int dayCount: backend.calendarView === "day" ? 1 : 7
    property var visibleEvents: backend.events
    property var agendaEvents: {
        let seen = {}
        return visibleEvents.filter(function(e) { if (seen[e.id]) return false; seen[e.id] = true; return true }).sort(function(a, b) {
            if (a.date !== b.date) return a.date < b.date ? -1 : 1
            if (a.all_day !== b.all_day) return a.all_day ? -1 : 1
            if (a.start_hour !== b.start_hour) return a.start_hour - b.start_hour
            return a.id < b.id ? -1 : a.id > b.id ? 1 : 0
        })
    }
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
    property bool shortcutsEnabled: visible && !editingField() && !popupOpen
    function editingField() {
        let item = Window.window ? Window.window.activeFocusItem : null
        while (item) { if (item.blocksWorkspaceShortcuts || (item.cursorPosition !== undefined && item.text !== undefined)) return true; item = item.parent }
        return false
    }
    function dateAt(index) { let date = new Date(backend.calendarFrom + "T12:00:00"); date.setDate(date.getDate() + index); return date }
    function isoAt(index) { return Qt.formatDate(dateAt(index), "yyyy-MM-dd") }
    function weekNumber() { let date = dateAt(3); let utc = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())); utc.setUTCDate(utc.getUTCDate() + 4 - (utc.getUTCDay() || 7)); return Math.ceil((((utc - new Date(Date.UTC(utc.getUTCFullYear(), 0, 1))) / 86400000) + 1) / 7) }
    function rangeTitle() { return Qt.formatDate(dateAt(0), dateAt(0).getMonth() === dateAt(6).getMonth() ? "d" : "d MMM") + " – " + Qt.formatDate(dateAt(6), "d MMMM") }
    function eventsAt(index) { return visibleEvents.filter(function(e) { return e.all_day ? index >= e.day_index && index < e.day_index + e.span_days : e.day_index === index }) }
    function allDayAt(index) { return eventsAt(index).filter(function(e) { return e.all_day }) }
    function openDay(index) { backend.setCalendarDate(isoAt(index)); backend.setCalendarView("day") }
    function showEvent(event, sourceItem) { if (!backend.calendarReady || !backend.connected) return; if (editor.opened) { editor.contentItem.forceActiveFocus(); return } details.openAt(event, sourceItem || calendar) }
    function createEvent() { if (editor.opened) { editor.contentItem.forceActiveFocus(); return } editor.editEvent = ({}); editor.openForDate(backend.calendarDate) }
    function createAt(index, hour) {
        if (!backend.connected || !backend.calendarSources.some(function(s) { return s.writable })) return
        if (editor.opened && (editor.editEvent.event_id || editor.saving || editor.unknownOutcome || editor.staleRevision)) { editor.contentItem.forceActiveFocus(); return }
        if (!editor.opened) { editor.editEvent = ({}); editor.openForDate(isoAt(index)) }
        editor.setSlot(isoAt(index), hour)
    }
    function sourceRule(event) { return event.busy_only || event.account !== backend.account ? Theme.inkAlpha(.5) : Theme.text }
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
    onHourHeightChanged: Qt.callLater(preserveGridPosition)
    onVisibleChanged: if (!visible) { details.close(); datePicker.close(); moreMenu.close(); allDayOverflow.close() }
    Shortcut { sequence: "D"; enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("day") }
    Shortcut { sequence: "W"; enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("week") }
    Shortcut { sequence: "M"; enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("month") }
    Shortcut { sequence: "A"; enabled: calendar.shortcutsEnabled; onActivated: backend.setCalendarView("agenda") }
    Shortcut { sequence: "N"; enabled: calendar.shortcutsEnabled; onActivated: calendar.createEvent() }
    Shortcut { sequence: "Left"; enabled: calendar.shortcutsEnabled; onActivated: backend.moveCalendar(-1) }
    Shortcut { sequence: "Right"; enabled: calendar.shortcutsEnabled; onActivated: backend.moveCalendar(1) }
    Shortcut { sequence: "T"; enabled: calendar.shortcutsEnabled; onActivated: backend.showCalendarToday() }

    ColumnLayout {
        anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 24; spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.topMargin: 24; Layout.bottomMargin: 16; spacing: 12
            Text { text: backend.calendarView === "week" ? "Week " + calendar.weekNumber() : backend.calendarView === "agenda" ? "Agenda" : Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), backend.calendarView === "day" ? "dddd d MMMM" : "MMMM yyyy"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(22); font.weight: Font.Medium }
            Text { text: (backend.calendarView === "week" ? calendar.rangeTitle() : calendar.agendaEvents.length + " events") + " · Combined"; color: Theme.subtle; font.pixelSize: Theme.fontSize(13); visible: calendar.width > 1050; Layout.fillWidth: true; elide: Text.ElideRight }
            Item { Layout.fillWidth: true; visible: calendar.width <= 1050 }
            Rectangle {
                implicitWidth: views.width + 4; implicitHeight: 32; color: Theme.surface; radius: 6
                Row { id: views; x: 2; y: 2; spacing: 2
                    Repeater { model: ["day", "week", "month"]
                        FmButton { required property string modelData; text: modelData.charAt(0).toUpperCase() + modelData.slice(1); selected: backend.calendarView === modelData; height: 28; font.pixelSize: Theme.fontSize(12); onClicked: backend.setCalendarView(modelData) }
                    }
                }
            }
            Rectangle {
                id: navigation; implicitWidth: 205; implicitHeight: 30; radius: 5; color: "transparent"; border.color: Theme.controlBorder
                Row {
                    anchors.fill: parent
                    FmButton { iconName: "previous"; width: 30; height: 30; tip: "Previous — Left"; onClicked: backend.moveCalendar(-1) }
                    Item { width: 1; height: 30; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: Theme.ruleWidth; height: parent.height; color: Theme.controlBorder } }
                    FmButton {
                        id: dateButton; objectName: "calendarDatePickerButton"; width: 143; height: 30; font.pixelSize: Theme.fontSize(12); iconName: "down"; iconSize: 10
                        text: backend.calendarView === "week" ? Qt.formatDate(calendar.dateAt(0), "d") + " – " + Qt.formatDate(calendar.dateAt(6), "d MMM") : backend.calendarView === "month" ? Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "MMM yyyy") : backend.calendarDate === backend.todayDate ? "Today" : Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), "ddd d MMM")
                        selected: datePicker.opened; tip: "Pick a date — T for today"
                        onClicked: { datePicker.month = new Date(backend.calendarDate + "T12:00:00"); datePicker.opened ? datePicker.close() : datePicker.open() }
                    }
                    Item { width: 1; height: 30; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: Theme.ruleWidth; height: parent.height; color: Theme.controlBorder } }
                    FmButton { iconName: "next"; width: 30; height: 30; tip: "Next — Right"; onClicked: backend.moveCalendar(1) }
                }
            }
            FmButton { id: moreButton; objectName: "calendarMoreButton"; iconName: "more"; implicitWidth: 28; implicitHeight: 28; tip: "Calendar options and coverage"; onClicked: moreMenu.open() }
        }
        Text { visible: !backend.calendarReady; text: backend.calendarCoverage; Layout.fillWidth: true; Layout.bottomMargin: 8; font.pixelSize: Theme.fontSize(11); color: Theme.subtle; wrapMode: Text.Wrap }
        RowLayout {
            visible: backend.calendarView === "day" || backend.calendarView === "week"
            Layout.fillHeight: false; Layout.maximumHeight: implicitHeight
            Layout.fillWidth: true; Layout.leftMargin: 56; Layout.rightMargin: backend.calendarView === "day" && calendar.width > 850 ? 352 : 0; spacing: 0
            Repeater { model: calendar.dayCount
                ColumnLayout {
                    id: dayHeader; required property int index; property var allDay: calendar.allDayAt(index)
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1; spacing: 6
                    FmButton {
                        Layout.fillWidth: true; implicitHeight: 30; onClicked: calendar.openDay(dayHeader.index); tip: "Open day"
                        contentItem: Row { spacing: 8
                            Text { text: Qt.formatDate(calendar.dateAt(dayHeader.index), "d"); color: calendar.isoAt(dayHeader.index) === backend.todayDate ? Theme.accent : Theme.text; font.pixelSize: Theme.fontSize(20); font.weight: Font.Medium }
                            Text { text: Qt.formatDate(calendar.dateAt(dayHeader.index), "ddd"); color: Theme.subtle; font.pixelSize: Theme.fontSize(12); anchors.baseline: parent.children[0].baseline }
                        }
                    }
                    Repeater { model: dayHeader.allDay.slice(0, 2)
                        Rectangle {
                            required property var modelData; Layout.fillWidth: true; implicitHeight: 22; Layout.leftMargin: 3; Layout.rightMargin: 4; color: Theme.selected; radius: 3
                            Rectangle { width: 2; height: parent.height; color: calendar.sourceRule(parent.modelData) }
                            Text { anchors.fill: parent; anchors.leftMargin: 8; anchors.rightMargin: 4; text: modelData.title; color: Theme.text; font.pixelSize: Theme.fontSize(11); verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; textFormat: Text.PlainText }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: calendar.showEvent(parent.modelData, parent) }
                        }
                    }
                    FmButton { visible: dayHeader.allDay.length > 2; text: "+ " + (dayHeader.allDay.length - 2) + " all day"; implicitHeight: 20; font.pixelSize: Theme.fontSize(10); onClicked: { allDayOverflow.eventRows = dayHeader.allDay; allDayOverflow.open() } }
                    Item { Layout.fillHeight: true }
                    Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: calendar.isoAt(dayHeader.index) === backend.todayDate ? Theme.accent : Theme.line }
                }
            }
        }
        RowLayout {
            visible: backend.calendarView === "day" || backend.calendarView === "week"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; spacing: 32
            Flickable {
                id: gridScroll; objectName: "calendarGrid"; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: width; contentHeight: 24 * calendar.hourHeight + 10; boundsBehavior: Flickable.StopAtBounds
                Component.onCompleted: Qt.callLater(calendar.preserveGridPosition)
                onHeightChanged: Qt.callLater(calendar.preserveGridPosition)
                onContentYChanged: calendar.rememberGridPosition()
                onMovementEnded: { calendar.retainedTopHour = Math.max(0, contentY) / calendar.hourHeight; backend.remember("calendarY", contentY); Qt.callLater(calendar.preserveGridPosition) }
                ScrollBar.vertical: ScrollBar {}
                Repeater { model: 25
                    Text { required property int index; objectName: "calendarHour-" + index; x: 0; y: Math.max(0, index * calendar.hourHeight - (index === 24 ? height : index === 0 ? 0 : height / 2)); text: (index < 10 ? "0" : "") + index + ":00"; color: Theme.subtle; font.pixelSize: Theme.fontSize(10) }
                }
                Item {
                    id: grid; x: 56; width: gridScroll.width - 56; height: 24 * calendar.hourHeight; property real dayWidth: width / calendar.dayCount
                    Repeater { model: calendar.dayCount
                        Rectangle {
                            required property int index; x: index * grid.dayWidth; width: grid.dayWidth; height: grid.height; color: calendar.isoAt(index) === backend.todayDate ? Theme.bg1 : "transparent"
                            Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: Theme.ruleWidth; height: parent.height; color: Theme.line }
                            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: function(mouse) { calendar.createAt(parent.index, Math.max(0, Math.min(23.5, Math.floor(mouse.y / calendar.hourHeight * 2) / 2))) } }
                            Rectangle { visible: calendar.isoAt(index) === backend.todayDate; x: Theme.snapX(parent, 0); y: Theme.snapY(parent, backend.nowHour * calendar.hourHeight); z: 2; width: parent.width; height: Theme.ruleWidth; color: Theme.accent; Rectangle { x: -3; y: -3; width: 7; height: 7; rotation: 45; color: Theme.accent } }
                        }
                    }
                    Repeater { model: 24
                        Rectangle { required property int index; x: Theme.snapX(parent, 0); y: Theme.snapY(parent, index * calendar.hourHeight); width: grid.width; height: Theme.ruleWidth; color: Theme.bg5 }
                    }
                    Repeater { model: calendar.visibleEvents
                        Rectangle {
                            id: eventBox; required property var modelData
                            objectName: "calendarEvent-" + (modelData.event_id || modelData.id)
                            activeFocusOnTab: visible && backend.calendarReady
                            Keys.onReturnPressed: calendar.showEvent(modelData, eventBox)
                            Keys.onSpacePressed: calendar.showEvent(modelData, eventBox)
                            border.width: activeFocus ? 1 : 0; border.color: Theme.accent
                            visible: !modelData.all_day && modelData.day_index >= 0 && modelData.day_index < calendar.dayCount && !(editor.opened && editor.preview.eventId === modelData.event_id)
                            x: modelData.day_index * grid.dayWidth + 3 + (modelData.column || 0) * (grid.dayWidth - 7) / (modelData.columns || 1); y: modelData.start_hour * calendar.hourHeight
                            width: (grid.dayWidth - 7) / (modelData.columns || 1) - ((modelData.columns || 1) > 1 ? 3 : 0); height: Math.max(20, modelData.duration_hours * calendar.hourHeight - 1)
                            radius: 3; color: eventMouse.containsMouse ? Theme.hover : Theme.selected; clip: true
                            Rectangle { width: 2; height: parent.height; color: calendar.sourceRule(eventBox.modelData) }
                            Row {
                                anchors.top: parent.top; anchors.topMargin: 3; anchors.left: parent.left; anchors.leftMargin: calendar.dayCount === 1 ? 12 : 7; anchors.right: parent.right; anchors.rightMargin: 4; spacing: calendar.dayCount === 1 ? 14 : 6
                                Text { visible: eventBox.width >= 110 || calendar.dayCount === 1; text: eventBox.modelData.time_display + (calendar.dayCount === 1 ? " – " + eventBox.modelData.end_display : ""); color: Theme.subtle; font.pixelSize: Theme.fontSize(calendar.dayCount === 1 ? 11 : 10) }
                                Text { text: eventBox.modelData.title; textFormat: Text.PlainText; color: Theme.text; font.pixelSize: Theme.fontSize(calendar.dayCount === 1 ? 13 : 12); font.weight: Font.Medium; width: Math.max(0, parent.width - (parent.children[0].visible ? parent.children[0].width + parent.spacing : 0)); elide: Text.ElideRight }
                            }
                            MouseArea { id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { eventBox.forceActiveFocus(); calendar.showEvent(eventBox.modelData, eventBox) } }
                            FmToolTip { visible: eventMouse.containsMouse; text: eventBox.modelData.title + " · " + eventBox.modelData.start_display + "\n" + eventBox.modelData.account_name + " / " + eventBox.modelData.calendar_name }
                            Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                        }
                    }
                    Rectangle {
                        property int day: Math.round((new Date(editor.preview.date + "T12:00:00") - calendar.dateAt(0)) / 86400000)
                        visible: editor.opened && !editor.preview.allDay && day >= 0 && day < calendar.dayCount && isFinite(editor.preview.startHour) && isFinite(editor.preview.endHour)
                        x: day * grid.dayWidth + 3; y: editor.preview.startHour * calendar.hourHeight; width: grid.dayWidth - 7
                        height: Math.max(20, (editor.preview.endHour - editor.preview.startHour) * calendar.hourHeight); color: Theme.accentAlpha(.125); radius: 3
                        Repeater { model: Math.max(0, Math.ceil(parent.width / 10)); Rectangle { required property int index; x: Theme.snapX(parent, index * 10); y: Theme.snapY(parent, 0); width: Math.max(0, Math.min(5, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                        Repeater { model: Math.max(0, Math.ceil(parent.width / 10)); Rectangle { required property int index; x: Theme.snapX(parent, index * 10); y: Theme.snapY(parent, parent.height - 1); width: Math.max(0, Math.min(5, parent.width - x)); height: Theme.ruleWidth; color: Theme.accent } }
                        Rectangle { width: 2; height: parent.height; color: Theme.accent }
                        Text { anchors.fill: parent; anchors.margins: 7; text: editor.preview.title; color: Theme.accent; font.pixelSize: Theme.fontSize(12); elide: Text.ElideRight; textFormat: Text.PlainText }
                    }
                }
            }
            ListView {
                visible: backend.calendarView === "day" && calendar.width > 850; Layout.preferredWidth: 320; Layout.fillHeight: true; clip: true; model: calendar.agendaEvents
                delegate: ItemDelegate {
                    required property var modelData; width: ListView.view.width; height: 72; onClicked: calendar.showEvent(modelData, this)
                    background: Rectangle { color: parent.hovered ? Theme.surface : "transparent"; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
                    contentItem: RowLayout {
                        spacing: 14
                        Text { text: modelData.all_day ? "All day" : modelData.time_display + " – " + modelData.end_display; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); Layout.preferredWidth: 92; Layout.alignment: Qt.AlignTop }
                        Column {
                            Layout.fillWidth: true; spacing: 3
                            Text { text: modelData.title; textFormat: Text.PlainText; color: Theme.text; font.pixelSize: Theme.fontSize(13); font.weight: Font.Medium; width: parent.width; elide: Text.ElideRight }
                            Text { text: [modelData.account_name, modelData.location, modelData.conference_url].filter(function(s) { return !!s }).join(" · "); color: Theme.subtle; font.pixelSize: Theme.fontSize(11); width: parent.width; elide: Text.ElideRight; textFormat: Text.PlainText }
                        }
                    }
                }
            }
        }
        GridLayout {
            visible: backend.calendarView === "month"; Layout.fillWidth: true; columns: 7; columnSpacing: 0
            Repeater { model: 7; Text { required property int index; Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.leftMargin: 10; Layout.bottomMargin: 8; text: Qt.formatDate(calendar.dateAt(index), "ddd"); color: Theme.subtle; font.pixelSize: Theme.fontSize(12) } }
        }
        GridLayout {
            visible: backend.calendarView === "month"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; columns: 7; columnSpacing: 0; rowSpacing: 0
            Repeater { model: calendar.monthCount
                Rectangle {
                    id: monthCell; required property int index; property var dayEvents: calendar.eventsAt(index); property bool inMonth: calendar.dateAt(index).getMonth() === new Date(backend.calendarDate + "T12:00:00").getMonth(); property int shown: Math.min(3, Math.max(0, Math.floor((height - 36) / 21) - (dayEvents.length * 21 > height - 36 ? 1 : 0)))
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1; Layout.preferredHeight: 1; clip: true
                    color: calendar.isoAt(index) === backend.todayDate ? Theme.bg1 : "transparent"; border.width: 1; border.color: Theme.line
                    Column { x: 8; y: 5; width: parent.width - 16; spacing: 3
                        FmButton { objectName: "calendarMonthDate-" + monthCell.index; text: Qt.formatDate(calendar.dateAt(monthCell.index), "d"); height: 24; width: 30; leftPadding: 0; rightPadding: 0; font.pixelSize: Theme.fontSize(13); foreground: calendar.isoAt(monthCell.index) === backend.todayDate ? Theme.accent : monthCell.inMonth ? Theme.text : Theme.inkAlpha(.5); onClicked: calendar.openDay(monthCell.index) }
                        Repeater { model: monthCell.dayEvents.slice(0, monthCell.shown)
                            Rectangle {
                                required property var modelData; width: monthCell.width - 16; height: 18; color: "transparent"
                                Rectangle { width: 2; height: parent.height; color: calendar.sourceRule(parent.modelData); opacity: monthCell.inMonth ? 1 : .5 }
                                FmButton { anchors.fill: parent; leftPadding: 7; rightPadding: 0; text: (parent.modelData.all_day ? "" : parent.modelData.time_display + "  ") + parent.modelData.title; foreground: monthCell.inMonth ? Theme.text : Theme.subtle; font.pixelSize: Theme.fontSize(11); textAlignment: Text.AlignLeft; tip: parent.modelData.title; onClicked: calendar.showEvent(parent.modelData, parent) }
                            }
                        }
                        FmButton { visible: monthCell.dayEvents.length > monthCell.shown; text: "+ " + (monthCell.dayEvents.length - monthCell.shown) + " more"; height: 18; font.pixelSize: Theme.fontSize(10); onClicked: calendar.openDay(monthCell.index) }
                    }
                }
            }
        }
        ListView {
            id: agenda; objectName: "calendarAgenda"; visible: backend.calendarView === "agenda"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; model: calendar.agendaEvents; clip: true
            delegate: ItemDelegate {
                required property var modelData; objectName: modelData.occurrence_selector ? "calendarAgendaOccurrence-" + modelData.id : "calendarAgendaEvent-" + modelData.account + "-" + modelData.event_id; width: agenda.width; height: 70; onClicked: calendar.showEvent(modelData, this)
                background: Rectangle { color: parent.hovered ? Theme.surface : "transparent"; Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, parent.height - 1); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
                contentItem: RowLayout { spacing: 18
                    Text { text: modelData.start_display; color: Theme.subtle; font.pixelSize: Theme.fontSize(12); Layout.preferredWidth: 200 }
                    Column { Layout.fillWidth: true; spacing: 4
                        Text { text: modelData.title; color: Theme.text; font.pixelSize: Theme.fontSize(13); textFormat: Text.PlainText }
                        Text { text: modelData.account_name + " / " + modelData.calendar_name + (modelData.location ? " · " + modelData.location : ""); color: Theme.subtle; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText }
                    }
                }
            }
            Text { anchors.centerIn: parent; visible: agenda.count === 0; text: backend.calendarReady ? "Nothing scheduled in this range." : backend.calendarCoverage; color: Theme.subtle; font.pixelSize: Theme.fontSize(13) }
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
                Text { text: Qt.formatDate(datePicker.month, backend.calendarView === "month" ? "yyyy" : "MMMM yyyy"); color: Theme.text; font.pixelSize: Theme.fontSize(13); font.weight: Font.Medium; Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter }
                FmButton { iconName: "next"; implicitWidth: 26; implicitHeight: 26; onClicked: datePicker.month = new Date(datePicker.month.getFullYear() + (backend.calendarView === "month" ? 1 : 0), datePicker.month.getMonth() + (backend.calendarView === "month" ? 0 : 1), 1) }
            }
            GridLayout {
                visible: backend.calendarView !== "month"; Layout.fillWidth: true; columns: 7; rowSpacing: 2; columnSpacing: 0
                Repeater { model: 7; Text { required property int index; text: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][(index + datePicker.weekStart) % 7]; color: Theme.subtle; font.pixelSize: Theme.fontSize(10); Layout.fillWidth: true; horizontalAlignment: Text.AlignHCenter } }
                Repeater { model: datePicker.days
                    FmButton {
                        required property int index; property date day: datePicker.dateFor(index); property string iso: Qt.formatDate(day, "yyyy-MM-dd")
                        text: day.getDate(); Layout.fillWidth: true; Layout.preferredWidth: 1; implicitHeight: 30; leftPadding: 0; rightPadding: 0; font.pixelSize: Theme.fontSize(12)
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
                Repeater { model: 12
                    FmButton { required property int index; text: Qt.formatDate(new Date(datePicker.month.getFullYear(), index, 1), "MMM"); Layout.fillWidth: true; implicitHeight: 34; selected: index === new Date(backend.calendarDate + "T12:00:00").getMonth() && datePicker.month.getFullYear() === new Date(backend.calendarDate + "T12:00:00").getFullYear(); onClicked: datePicker.select(new Date(datePicker.month.getFullYear(), index, 1, 12)) }
                }
            }
            Text { visible: backend.calendarView === "week"; text: "Pick any day — the whole week opens"; color: Theme.subtle; font.pixelSize: Theme.fontSize(10); Layout.alignment: Qt.AlignHCenter }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            RowLayout { Layout.fillWidth: true
                FmButton { text: "Today"; foreground: Theme.accent; implicitHeight: 24; font.pixelSize: Theme.fontSize(12); onClicked: { backend.showCalendarToday(); datePicker.close() } }
                Text { text: backend.preferences.calendar_display_time_zone || "Time zone unavailable"; color: Theme.subtle; font.pixelSize: Theme.fontSize(10); Layout.fillWidth: true; horizontalAlignment: Text.AlignRight }
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
            Text { text: "Combined · " + (backend.preferences.calendar_display_time_zone || "Time zone unavailable") + "\n" + backend.calendarCoverage; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
        }
    }
    EventDetails { id: details; onEditRequested: function(event) { editor.editEvent = event; editor.openForDate(event.start.slice(0, 10)) } }
    EventEditor { id: editor }
    CalendarOperations { id: operationHistory }
    CalendarEventList { id: allDayOverflow; title: "All-day events"; available: backend.calendarReady && backend.connected; onEventSelected: function(event) { calendar.showEvent(event) } }
}
