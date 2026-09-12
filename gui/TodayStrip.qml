import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: timeline
    color: Theme.canvas
    signal openCalendar()
    property var allDayEvents: backend.todayEvents.filter(function(e) { return e.all_day })
    property var timedEvents: backend.todayEvents.filter(function(e) { return !e.all_day })
    property bool expandedAllDay: false
    property int preferredStart: backend.preferences.timeline_start_hour === undefined ? 7 : backend.preferences.timeline_start_hour
    property int preferredEnd: backend.preferences.timeline_end_hour === undefined ? 21 : backend.preferences.timeline_end_hour
    property real firstHour: Math.min(preferredStart, ...timedEvents.map(function(e) { return Math.floor(e.start_hour) }))
    property real lastHour: Math.max(preferredEnd, ...timedEvents.map(function(e) { return Math.ceil(e.start_hour + e.duration_hours) }))
    property int lanes: Math.max(1, ...timedEvents.map(function(e) { return (e.column || 0) + 1 }))
    implicitHeight: 76 + Math.min(2, lanes) * 22 + (expandedAllDay ? Math.min(78, Math.ceil(allDayEvents.length / 3) * 26) : 0)
    function showEvent(event) { if (!backend.timelineReady || !backend.connected) return; details.eventData = event; details.open() }
    Rectangle { width: parent.width; height: 1; color: Theme.line }
    ColumnLayout {
        anchors.fill: parent; anchors.leftMargin: 24; anchors.rightMargin: 24; anchors.topMargin: 9; anchors.bottomMargin: 10; spacing: 7
        RowLayout {
            Layout.fillWidth: true; spacing: 7
            FmButton { objectName: "timelinePreviousDay"; iconName: "previous"; implicitWidth: 25; implicitHeight: 26; tip: "Previous day"; onClicked: backend.moveTimeline(-1) }
            FmButton { objectName: "timelineNextDay"; iconName: "next"; implicitWidth: 25; implicitHeight: 26; tip: "Next day"; onClicked: backend.moveTimeline(1) }
            Text { objectName: "timelineDateLabel"; text: Qt.formatDate(new Date(backend.timelineDate + "T12:00:00"), "ddd d MMM"); color: backend.timelineDate === backend.todayDate ? Theme.text : Theme.muted; font.pixelSize: 12; font.weight: Font.Medium }
            Text { text: "America/Toronto"; color: Theme.subtle; font.pixelSize: 10 }
            FmButton { objectName: "timelineToday"; visible: backend.timelineDate !== backend.todayDate; text: "Back to today"; implicitHeight: 26; font.pixelSize: 11; onClicked: backend.showTimelineToday() }
            Item { Layout.fillWidth: true }
            FmButton { visible: timeline.allDayEvents.length > 0; text: timeline.allDayEvents.length + " all day"; selected: timeline.expandedAllDay; font.pixelSize: 11; implicitHeight: 26; onClicked: timeline.expandedAllDay = !timeline.expandedAllDay }
            FmButton { visible: timeline.lanes > 2; text: "All " + backend.todayEvents.length + " events"; font.pixelSize: 11; implicitHeight: 26; tip: "Show every overlapping event in this day"; enabled: backend.timelineReady; onClicked: dayEvents.open() }
            Text { text: backend.timelineCoverage; color: Theme.subtle; font.pixelSize: 10; visible: timeline.width > 800; Layout.maximumWidth: timeline.width * 0.29; elide: Text.ElideRight }
            FmButton { text: "Open calendar ↗"; implicitHeight: 26; font.pixelSize: 11; onClicked: timeline.openCalendar() }
        }
        ScrollView {
            visible: timeline.expandedAllDay; Layout.fillWidth: true; Layout.preferredHeight: Math.min(78, Math.ceil(timeline.allDayEvents.length / 3) * 26); clip: true; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            GridLayout {
                width: timeline.width - 48; columns: 3; rowSpacing: 3; columnSpacing: 6
                Repeater { model: timeline.allDayEvents
                    FmButton { required property var modelData; text: modelData.title + " · " + modelData.account_name; textAlignment: Text.AlignLeft; selected: true; implicitHeight: 23; Layout.fillWidth: true; Layout.preferredWidth: 1; font.pixelSize: 10; onClicked: timeline.showEvent(modelData) }
                }
            }
        }
        Flickable {
            id: trackScroll; Layout.fillWidth: true; Layout.fillHeight: true; contentWidth: width; contentHeight: timeline.lanes * 22 + 24; clip: true; boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { }
            Item {
                id: track; width: trackScroll.width; height: trackScroll.contentHeight
                MouseArea {
                    anchors.fill: parent; property real pressX: 0
                    onPressed: function(mouse) { pressX = mouse.x }
                    onReleased: function(mouse) { let distance = mouse.x - pressX; if (Math.abs(distance) > 65) backend.moveTimeline(distance > 0 ? -1 : 1) }
                }
                Repeater {
                    model: Math.ceil(timeline.lastHour - timeline.firstHour) + 1
                    Item {
                        required property int index
                        x: index / (timeline.lastHour - timeline.firstHour) * track.width; height: track.height
                        Rectangle { y: 17; width: 1; height: track.height - 17; color: Theme.line; opacity: 0.65 }
                        Text { text: (timeline.firstHour + index < 10 ? "0" : "") + (timeline.firstHour + index) + ":00"; x: index === 0 ? 0 : index === Math.ceil(timeline.lastHour - timeline.firstHour) ? -implicitWidth : -implicitWidth / 2; color: Theme.subtle; font.pixelSize: 9 }
                    }
                }
                Repeater {
                    model: timeline.timedEvents
                    Rectangle {
                        id: durationBox; required property var modelData
                        activeFocusOnTab: backend.timelineReady
                        Keys.onReturnPressed: timeline.showEvent(modelData)
                        Keys.onSpacePressed: timeline.showEvent(modelData)
                        border.width: activeFocus ? 1 : 0; border.color: Theme.accent
                        x: (modelData.start_hour - timeline.firstHour) / (timeline.lastHour - timeline.firstHour) * track.width
                        y: 21 + (modelData.column || 0) * 22
                        width: Math.max(6, modelData.duration_hours / (timeline.lastHour - timeline.firstHour) * track.width - 3); height: 21
                        color: eventMouse.containsMouse ? Theme.hover : Theme.selected; radius: 4; clip: true
                        Rectangle { width: 2; height: parent.height; color: durationBox.modelData.busy_only ? Theme.subtle : Theme.accent }
                        Text { anchors.fill: parent; anchors.leftMargin: 7; anchors.rightMargin: 4; text: durationBox.modelData.title; color: Theme.muted; font.pixelSize: 10; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight; textFormat: Text.PlainText }
                        MouseArea {
                            id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; property real pressX: 0
                            onPressed: function(mouse) { pressX = mouse.x }
                            onReleased: function(mouse) { let distance = mouse.x - pressX; if (Math.abs(distance) > 65) backend.moveTimeline(distance > 0 ? -1 : 1); else timeline.showEvent(durationBox.modelData) }
                        }
                        FmToolTip { visible: eventMouse.containsMouse; text: durationBox.modelData.title + "\n" + durationBox.modelData.time_display + " — " + durationBox.modelData.end_display + "\n" + durationBox.modelData.account_name + " / " + durationBox.modelData.calendar_name }
                        Accessible.role: Accessible.Button; Accessible.name: modelData.title + ", " + modelData.start_display
                    }
                }
                Rectangle {
                    visible: backend.timelineDate === backend.todayDate && backend.nowHour >= timeline.firstHour && backend.nowHour <= timeline.lastHour
                    x: (backend.nowHour - timeline.firstHour) / (timeline.lastHour - timeline.firstHour) * track.width; y: 15; width: 1; height: track.height - 15; color: Theme.accent
                    Rectangle { x: -3; y: -2; width: 7; height: 7; radius: 4; color: Theme.accent }
                }
                Text { anchors.centerIn: parent; visible: backend.todayEvents.length === 0; text: backend.timelineReady ? "No cached events for this day" : backend.timelineCoverage; color: Theme.subtle; font.pixelSize: 11; width: parent.width; horizontalAlignment: Text.AlignHCenter; elide: Text.ElideRight }
            }
        }
    }
    EventDetails { id: details; timelineSource: true; onEditRequested: function(event) { editor.editEvent = event; editor.openForDate(event.start.slice(0, 10)) } }
    EventEditor { id: editor }
    CalendarEventList { id: dayEvents; title: Qt.formatDate(new Date(backend.timelineDate + "T12:00:00"), "ddd d MMM") + " · all events"; eventRows: backend.todayEvents; available: backend.timelineReady && backend.connected; onEventSelected: function(event) { timeline.showEvent(event) } }
}
