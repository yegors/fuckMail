import QtQuick
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
    property var allDayEvents: visibleEvents.filter(function(e) { return e.all_day })
    property int allDayLanes: allDayEvents.reduce(function(n, e) { return Math.max(n, e.lane + 1) }, 0)
    property int shownAllDayLanes: Math.min(2, allDayLanes)
    property int startHour: backend.preferences.calendar_start_hour === undefined ? 7 : backend.preferences.calendar_start_hour
    property int endHour: backend.preferences.calendar_end_hour === undefined ? 21 : backend.preferences.calendar_end_hour
    property real hourHeight: Math.max(16, (gridScroll.height - 16) / Math.max(1, endHour - startHour))
    onStartHourChanged: gridScroll.contentY = startHour * hourHeight
    onHourHeightChanged: if (!gridScroll.moving) gridScroll.contentY = startHour * hourHeight
    function dateAt(index) { let date = new Date(backend.calendarFrom + "T12:00:00"); date.setDate(date.getDate() + index); return date }
    function isoAt(index) { return Qt.formatDate(dateAt(index), "yyyy-MM-dd") }
    function weekNumber() { let date = dateAt(3); let utc = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())); utc.setUTCDate(utc.getUTCDate() + 4 - (utc.getUTCDay() || 7)); return Math.ceil((((utc - new Date(Date.UTC(utc.getUTCFullYear(), 0, 1))) / 86400000) + 1) / 7) }
    function rangeTitle() { return Qt.formatDate(dateAt(0), dateAt(0).getMonth() === dateAt(6).getMonth() ? "d" : "d MMM") + "–" + Qt.formatDate(dateAt(6), "d MMMM yyyy") }
    function eventsAt(index) { return visibleEvents.filter(function(e) { return e.all_day ? index >= e.day_index && index < e.day_index + e.span_days : e.day_index === index }) }
    function openDay(index) { backend.setCalendarDate(isoAt(index)); backend.setCalendarView("day") }
    function showEvent(event) { if (!backend.calendarReady || !backend.connected) return; details.eventData = event; details.open() }
    function createEvent() { editor.editEvent = ({}); editor.openForDate(backend.calendarDate) }
    Shortcut { sequence: "D"; enabled: calendar.visible && !editor.opened && !details.opened && !operationHistory.opened && !allDayOverflow.opened; onActivated: backend.setCalendarView("day") }
    Shortcut { sequence: "W"; enabled: calendar.visible && !editor.opened && !details.opened && !operationHistory.opened && !allDayOverflow.opened; onActivated: backend.setCalendarView("week") }
    Shortcut { sequence: "M"; enabled: calendar.visible && !editor.opened && !details.opened && !operationHistory.opened && !allDayOverflow.opened; onActivated: backend.setCalendarView("month") }
    Shortcut { sequence: "A"; enabled: calendar.visible && !editor.opened && !details.opened && !operationHistory.opened && !allDayOverflow.opened; onActivated: backend.setCalendarView("agenda") }
    Shortcut { sequence: "Left"; enabled: calendar.visible && !editor.opened && !details.opened && !operationHistory.opened && !allDayOverflow.opened; onActivated: backend.moveCalendar(-1) }
    Shortcut { sequence: "Right"; enabled: calendar.visible && !editor.opened && !details.opened && !operationHistory.opened && !allDayOverflow.opened; onActivated: backend.moveCalendar(1) }
    Shortcut { sequence: "T"; enabled: calendar.visible && !editor.opened && !details.opened && !operationHistory.opened && !allDayOverflow.opened; onActivated: backend.showCalendarToday() }
    ColumnLayout {
        anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 24; spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.topMargin: 24; Layout.bottomMargin: 16; spacing: 12
            Text { text: backend.calendarView === "week" ? "Week " + calendar.weekNumber() : backend.calendarView === "agenda" ? "Agenda" : Qt.formatDate(new Date(backend.calendarDate + "T12:00:00"), backend.calendarView === "day" ? "d MMMM yyyy" : "MMMM yyyy"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: 22; font.weight: Font.Medium }
            Text { text: backend.calendarView === "week" ? calendar.rangeTitle() : "Combined calendar"; color: Theme.muted; font.pixelSize: 12; visible: calendar.width > 1050 }
            Item { Layout.fillWidth: true }
            Rectangle {
                implicitWidth: views.width + 4; implicitHeight: 34; color: Theme.surface; radius: 6
                Row { id: views; x: 2; y: 2
                    Repeater { model: ["day", "week", "month", "agenda"]
                        FmButton { required property string modelData; text: modelData.charAt(0).toUpperCase() + modelData.slice(1); selected: backend.calendarView === modelData; height: 30; font.pixelSize: 12; onClicked: backend.setCalendarView(modelData) }
                    }
                }
            }
            Row { spacing: 0
                FmButton { text: "‹"; outlined: true; width: 30; tip: "Previous period"; onClicked: backend.moveCalendar(-1) }
                FmButton { text: "Today"; outlined: true; onClicked: backend.showCalendarToday() }
                FmButton { text: "›"; outlined: true; width: 30; tip: "Next period"; onClicked: backend.moveCalendar(1) }
            }
            FmButton { objectName: "newEventButton"; text: "New event"; prominent: true; enabled: backend.connected && backend.calendarSources.some(function(s) { return s.writable }); onClicked: calendar.createEvent() }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.bottomMargin: 14; spacing: 12
            Text { text: (calendar.width <= 1050 && backend.calendarView === "week" ? calendar.rangeTitle() + " · " : "") + "America/Toronto"; color: Theme.subtle; font.pixelSize: 11 }
            Text { text: backend.calendarCoverage; color: Theme.subtle; font.pixelSize: 11; Layout.fillWidth: true; elide: Text.ElideRight }
            FmButton { objectName: "calendarActivityButton"; text: "Local activity"; iconName: "clock"; implicitHeight: 25; font.pixelSize: 11; tip: "Inspect or retry recorded calendar changes"; onClicked: operationHistory.open() }
        }
        RowLayout {
            visible: backend.calendarView === "day" || backend.calendarView === "week"; Layout.fillWidth: true; Layout.leftMargin: 56; Layout.rightMargin: backend.calendarView === "day" && calendar.width > 850 ? 311 : 0; spacing: 0
            Repeater { model: calendar.dayCount
                Item {
                    required property int index; Layout.fillWidth: true; Layout.preferredWidth: 1; implicitHeight: 37
                    Row { x: 10; spacing: 8
                        Text { text: Qt.formatDate(calendar.dateAt(index), "d"); color: calendar.isoAt(index) === backend.todayDate ? Theme.accent : Theme.text; font.pixelSize: 20; font.weight: Font.Medium }
                        Text { text: Qt.formatDate(calendar.dateAt(index), "ddd"); color: Theme.muted; font.pixelSize: 12; anchors.baseline: parent.children[0].baseline }
                    }
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: calendar.isoAt(index) === backend.todayDate ? Theme.accent : Theme.line }
                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: calendar.openDay(index) }
                }
            }
        }
        Item {
            visible: (backend.calendarView === "day" || backend.calendarView === "week") && calendar.allDayLanes > 0
            Layout.fillWidth: true; Layout.leftMargin: 56; Layout.rightMargin: backend.calendarView === "day" && calendar.width > 850 ? 311 : 0
            implicitHeight: calendar.shownAllDayLanes * 27 + (calendar.allDayLanes > 2 ? 25 : 4)
            Text { x: -56; y: 9; text: "All day"; color: Theme.subtle; font.pixelSize: 10 }
            Repeater { model: calendar.allDayEvents
                FmButton {
                    required property var modelData; visible: modelData.lane < calendar.shownAllDayLanes
                    x: modelData.day_index * (parent.width / calendar.dayCount) + 4; y: modelData.lane * 27 + 3
                    width: modelData.span_days * parent.width / calendar.dayCount - 8; height: 23; text: modelData.title; selected: true; font.pixelSize: 11; textAlignment: Text.AlignLeft
                    onClicked: calendar.showEvent(modelData)
                }
            }
            FmButton { visible: calendar.allDayLanes > 2; y: calendar.shownAllDayLanes * 27; height: 25; text: "+ " + calendar.allDayEvents.filter(function(e) { return e.lane >= 2 }).length + " more all-day events"; font.pixelSize: 10; enabled: backend.calendarReady; onClicked: allDayOverflow.open() }
        }
        RowLayout {
            visible: backend.calendarView === "day" || backend.calendarView === "week"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; spacing: 26
            Flickable {
                id: gridScroll; objectName: "calendarGrid"; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; contentWidth: width; contentHeight: 24 * calendar.hourHeight + 16; boundsBehavior: Flickable.StopAtBounds
                Component.onCompleted: contentY = calendar.startHour * calendar.hourHeight
                onHeightChanged: if (!moving) contentY = calendar.startHour * calendar.hourHeight
                onMovementEnded: backend.remember("calendarY", contentY)
                ScrollBar.vertical: ScrollBar { }
                Repeater { model: 24
                    Text { required property int index; objectName: "calendarHour-" + index; x: 0; y: 1 + index * calendar.hourHeight; text: (index < 10 ? "0" : "") + index + ":00"; color: Theme.subtle; font.pixelSize: 11 }
                }
                Item {
                    id: grid; x: 56; y: 8; width: gridScroll.width - 56; height: 24 * calendar.hourHeight; property real dayWidth: width / calendar.dayCount
                    Repeater { model: calendar.dayCount
                        Rectangle {
                            required property int index; x: index * grid.dayWidth; width: grid.dayWidth; height: grid.height; color: calendar.isoAt(index) === backend.todayDate ? Theme.surface : "transparent"
                            Rectangle { width: 1; height: parent.height; color: Theme.line }
                            Rectangle { visible: calendar.isoAt(index) === backend.todayDate; y: backend.nowHour * calendar.hourHeight; z: 2; width: parent.width; height: 1; color: Theme.accent; Rectangle { x: -3; y: -3; width: 7; height: 7; radius: 4; color: Theme.accent } }
                        }
                    }
                    Repeater { model: 24
                        Item { required property int index; y: index * calendar.hourHeight; width: grid.width; height: 1
                            Rectangle { anchors.fill: parent; color: Theme.line; opacity: 0.65 }
                        }
                    }
                    Repeater { model: calendar.visibleEvents
                        Rectangle {
                            id: eventBox; required property var modelData
                            objectName: "calendarEvent-" + (modelData.event_id || modelData.id)
                            activeFocusOnTab: visible && backend.calendarReady
                            Keys.onReturnPressed: calendar.showEvent(modelData)
                            Keys.onSpacePressed: calendar.showEvent(modelData)
                            border.width: activeFocus ? 1 : 0; border.color: Theme.accent
                            visible: !modelData.all_day && modelData.day_index >= 0 && modelData.day_index < calendar.dayCount
                            x: modelData.day_index * grid.dayWidth + 4 + (modelData.column || 0) * (grid.dayWidth - 8) / (modelData.columns || 1); y: modelData.start_hour * calendar.hourHeight
                            width: (grid.dayWidth - 8) / (modelData.columns || 1) - ((modelData.columns || 1) > 1 ? 3 : 0); height: Math.max(15, modelData.duration_hours * calendar.hourHeight - 2)
                            radius: 4; color: eventMouse.containsMouse ? Theme.hover : Theme.selected; clip: true
                            Rectangle { width: 2; height: parent.height; color: eventBox.modelData.busy_only ? Theme.subtle : Theme.accent; opacity: 0.65 }
                            Column { anchors.fill: parent; anchors.margins: 8; spacing: 3; visible: eventBox.height > 50
                                Text { text: eventBox.modelData.time_display; color: Theme.muted; font.pixelSize: 11 }
                                Text { text: eventBox.modelData.title; textFormat: Text.PlainText; color: Theme.text; font.pixelSize: 12; font.weight: Font.Medium; width: parent.width; elide: Text.ElideRight }
                                Text { visible: eventBox.height > 65; text: eventBox.modelData.account_name; color: Theme.subtle; font.pixelSize: 10; width: parent.width; elide: Text.ElideRight }
                            }
                            Text { visible: eventBox.height <= 50; anchors.fill: parent; anchors.leftMargin: 7; anchors.rightMargin: 4; text: (eventBox.width >= 130 ? eventBox.modelData.time_display + "  " : "") + eventBox.modelData.title; textFormat: Text.PlainText; color: Theme.text; font.pixelSize: 10; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight }
                            MouseArea { id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { eventBox.forceActiveFocus(); calendar.showEvent(eventBox.modelData) } }
                            FmToolTip { visible: eventMouse.containsMouse; text: eventBox.modelData.title + " · " + eventBox.modelData.start_display + "\n" + eventBox.modelData.account_name + " / " + eventBox.modelData.calendar_name }
                            Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                        }
                    }
                }
            }
            ListView {
                visible: backend.calendarView === "day" && calendar.width > 850; Layout.preferredWidth: 285; Layout.fillHeight: true; clip: true; model: calendar.visibleEvents
                delegate: ItemDelegate {
                    required property var modelData; width: ListView.view.width; height: 78; onClicked: calendar.showEvent(modelData)
                    background: Rectangle { color: parent.hovered ? Theme.surface : "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line } }
                    contentItem: Column { spacing: 5
                        Text { text: modelData.time_display + "  " + (modelData.all_day ? "" : modelData.end_display); color: Theme.muted; font.pixelSize: 11 }
                        Text { text: modelData.title; textFormat: Text.PlainText; color: Theme.text; font.pixelSize: 13; width: parent.width; elide: Text.ElideRight }
                        Text { text: modelData.account_name + " / " + modelData.calendar_name; color: Theme.subtle; font.pixelSize: 11 }
                    }
                }
            }
        }
        GridLayout {
            visible: backend.calendarView === "month"; Layout.fillWidth: true; columns: 7; columnSpacing: 0; rowSpacing: 0
            Repeater { model: 7; Text { required property int index; Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.leftMargin: 10; Layout.bottomMargin: 8; text: Qt.formatDate(calendar.dateAt(index), "ddd"); color: Theme.muted; font.pixelSize: 12 } }
        }
        GridLayout {
            visible: backend.calendarView === "month"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; columns: 7; columnSpacing: 0; rowSpacing: 0
            Repeater { model: backend.calendarDays
                Rectangle {
                    id: monthCell; required property int index; property var dayEvents: calendar.eventsAt(index); property int shown: Math.max(0, Math.floor((height - 34) / 21) - (dayEvents.length * 21 > height - 34 ? 1 : 0))
                    Layout.fillWidth: true; Layout.fillHeight: true; Layout.preferredWidth: 1; Layout.preferredHeight: 1; clip: true
                    color: calendar.isoAt(index) === backend.todayDate ? Theme.surface : "transparent"; border.width: 1; border.color: Theme.line
                    Column { x: 8; y: 5; width: parent.width - 16; spacing: 3
                        FmButton { objectName: "calendarMonthDate-" + monthCell.index; text: Qt.formatDate(calendar.dateAt(monthCell.index), "d"); height: 24; width: 34; leftPadding: 4; rightPadding: 4; selected: calendar.isoAt(monthCell.index) === backend.todayDate; foreground: calendar.isoAt(monthCell.index) === backend.todayDate ? Theme.accent : Theme.muted; onClicked: calendar.openDay(monthCell.index) }
                        Repeater { model: monthCell.dayEvents.slice(0, monthCell.shown)
                            FmButton { required property var modelData; text: (modelData.all_day ? "" : modelData.time_display + "  ") + modelData.title; width: monthCell.width - 16; height: 18; font.pixelSize: 11; textAlignment: Text.AlignLeft; onClicked: calendar.showEvent(modelData) }
                        }
                        FmButton { visible: monthCell.dayEvents.length > monthCell.shown; text: "+ " + (monthCell.dayEvents.length - monthCell.shown) + " more"; height: 18; font.pixelSize: 10; onClicked: calendar.openDay(monthCell.index) }
                    }
                }
            }
        }
        ListView {
            id: agenda; visible: backend.calendarView === "agenda"; Layout.fillWidth: true; Layout.fillHeight: true; Layout.bottomMargin: 16; model: calendar.agendaEvents; clip: true; spacing: 0
            delegate: ItemDelegate {
                required property var modelData; width: agenda.width; height: 75; onClicked: calendar.showEvent(modelData)
                background: Rectangle { color: parent.hovered ? Theme.surface : "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line } }
                contentItem: RowLayout { spacing: 18
                    Text { text: modelData.start_display; color: Theme.muted; font.pixelSize: 12; Layout.preferredWidth: 200 }
                    Column { Layout.fillWidth: true; spacing: 6
                        Text { text: modelData.title; color: Theme.text; font.pixelSize: 14; textFormat: Text.PlainText }
                        Text { text: modelData.account_name + " / " + modelData.calendar_name + (modelData.location ? " · " + modelData.location : ""); color: Theme.subtle; font.pixelSize: 11; textFormat: Text.PlainText }
                    }
                    Text { text: modelData.busy_only ? "Limited details" : modelData.recurring ? "Recurring" : ""; color: Theme.subtle; font.pixelSize: 11 }
                }
            }
            Text { anchors.centerIn: parent; visible: agenda.count === 0; text: backend.calendarReady ? "No cached events in this date range." : backend.calendarCoverage; color: Theme.muted; font.pixelSize: 13 }
        }
    }
    EventDetails { id: details; onEditRequested: function(event) { editor.editEvent = event; editor.openForDate(event.start.slice(0, 10)) } }
    EventEditor { id: editor }
    CalendarOperations { id: operationHistory }
    CalendarEventList { id: allDayOverflow; title: "All-day events"; eventRows: calendar.allDayEvents; available: backend.calendarReady && backend.connected; onEventSelected: function(event) { calendar.showEvent(event) } }
}
