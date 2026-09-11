import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: footer
    implicitHeight: 102; color: "#0c0d0f"
    signal openCalendar()
    property var allDayEvents: backend.todayEvents.filter(function(e) { return e.all_day })
    property var timedEvents: backend.todayEvents.filter(function(e) { return !e.all_day })
    property real firstHour: Math.min(7, ...timedEvents.map(function(e) { return e.start_hour }))
    property real lastHour: Math.max(21, ...timedEvents.map(function(e) { return e.start_hour + e.duration_hours }))
    Rectangle { width: parent.width; height: 1; color: Theme.line }
    RowLayout {
        anchors.top: parent.top; anchors.left: parent.left; anchors.right: parent.right
        anchors.margins: 13; spacing: 9
        FmButton { text: "▦  Today"; font.pixelSize: 11; onClicked: footer.openCalendar(); tip: "Open calendar" }
        Text { text: backend.currentAccount.name || ""; color: Theme.text; font.pixelSize: 12; font.bold: true }
        Text { text: "Fri 11 Sep · America/Toronto"; color: Theme.muted; font.pixelSize: 11 }
        Text { text: footer.allDayEvents.map(function(e) { return e.title }).join(" · ") + " · all day"; visible: footer.allDayEvents.length > 0; color: Theme.muted; font.pixelSize: 11; Layout.maximumWidth: footer.width * 0.25; elide: Text.ElideRight; textFormat: Text.PlainText }
        Item { Layout.fillWidth: true }
        Text { text: backend.todayEvents.length + " fixture events · this account only"; color: Theme.subtle; font.pixelSize: 11 }
    }
    Item {
        id: line
        anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 25
        y: 62; height: 30
        Rectangle { y: 0; width: parent.width; height: 1; color: Theme.line }
        Text { y: 15; text: (footer.firstHour < 10 ? "0" : "") + Math.floor(footer.firstHour) + ":00"; color: Theme.subtle; font.pixelSize: 9 }
        Text { anchors.right: parent.right; y: 15; text: Math.ceil(footer.lastHour) + ":00"; color: Theme.subtle; font.pixelSize: 9 }
        Repeater {
            model: footer.timedEvents
            Item {
                id: marker
                required property var modelData
                x: (modelData.start_hour - footer.firstHour) / (footer.lastHour - footer.firstHour) * line.width
                width: Math.max(4, modelData.duration_hours / (footer.lastHour - footer.firstHour) * line.width); height: 29
                Rectangle { y: -1; width: parent.width; height: 3; color: Theme.subtle; radius: 1 }
                Rectangle { x: -3; y: -3; width: 7; height: 7; radius: 4; color: Theme.text }
                Text { y: 13 + ((marker.modelData.column || 0) % 2) * 12; text: marker.modelData.title; width: Math.min(140, parent.width + 70); elide: Text.ElideRight; color: Theme.muted; font.pixelSize: 10; textFormat: Text.PlainText }
                MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: footer.openCalendar() }
                FmToolTip { visible: hover.hovered; text: marker.modelData.title + " · " + marker.modelData.time_display }
                HoverHandler { id: hover }
            }
        }
        Text { anchors.centerIn: parent; visible: backend.todayEvents.length === 0; text: backend.connected ? "No events in this account for the fixture day" : "Calendar is waiting for the local engine"; color: Theme.subtle; font.pixelSize: 11 }
    }
}
