import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: calendar
    property real hourHeight: 57
    property var selectedEvent: ({})
    property var allDayEvents: backend.events.filter(function(e) { return e.all_day })
    property int allDayLanes: Math.max(0, ...allDayEvents.map(function(e) { return e.lane + 1 }))
    function dayDate(index) {
        let date = new Date(backend.weekStart + "T12:00:00")
        date.setDate(date.getDate() + index)
        return date
    }
    ColumnLayout {
        anchors.fill: parent; spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 20; spacing: 12
            Text { text: "Week " + Qt.formatDate(calendar.dayDate(0), "d MMM") + " — " + Qt.formatDate(calendar.dayDate(6), "d MMM"); color: Theme.text; font.pixelSize: 20; font.bold: true }
            Text { text: (backend.currentAccount.name || "") + "  ·  America/Toronto"; color: Theme.muted; font.pixelSize: 11 }
            Item { Layout.fillWidth: true }
            Text { text: "Read-only fixture week"; color: Theme.subtle; font.pixelSize: 11 }
            FmButton { text: "‹"; outlined: true; tip: "Previous week"; onClicked: backend.moveWeek(-1) }
            FmButton { text: "Fixture today"; outlined: true; onClicked: backend.showFixtureWeek() }
            FmButton { text: "›"; outlined: true; tip: "Next week"; onClicked: backend.moveWeek(1) }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.leftMargin: 62; Layout.rightMargin: 18; spacing: 0
            Repeater {
                model: 7
                Item {
                    required property int index
                    Layout.fillWidth: true; Layout.preferredWidth: 1; Layout.minimumWidth: 0; implicitHeight: 40
                    Row {
                        x: 11; spacing: 7
                        Text { text: Qt.formatDate(calendar.dayDate(index), "d"); color: Qt.formatDate(calendar.dayDate(index), "yyyy-MM-dd") === backend.fixtureToday ? Theme.accent : Theme.text; font.pixelSize: 19 }
                        Text { text: Qt.formatDate(calendar.dayDate(index), "ddd"); color: Theme.muted; font.pixelSize: 11; anchors.baseline: parent.children[0].baseline }
                    }
                }
            }
        }
        Item {
            Layout.fillWidth: true; Layout.leftMargin: 62; Layout.rightMargin: 18
            implicitHeight: calendar.allDayLanes * 31 + (calendar.allDayLanes ? 4 : 0)
            visible: calendar.allDayEvents.length > 0
            Text { x: -47; y: 8; text: "All day"; color: Theme.subtle; font.pixelSize: 10 }
            Repeater {
                model: calendar.allDayEvents
                FmButton {
                    required property var modelData
                    x: modelData.day_index * (parent.width / 7) + 4
                    y: modelData.lane * 31
                    width: modelData.span_days * parent.width / 7 - 8; height: 27
                    text: modelData.title; selected: true; font.pixelSize: 10
                    onClicked: { calendar.selectedEvent = modelData; details.open() }
                }
            }
        }
        ScrollView {
            Layout.fillWidth: true; Layout.fillHeight: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            Flickable {
                id: gridScroll; objectName: "calendarGrid"
                clip: true; contentWidth: width; contentHeight: 24 * calendar.hourHeight
                boundsBehavior: Flickable.StopAtBounds
                Component.onCompleted: contentY = 7 * calendar.hourHeight - 8
                onMovementEnded: backend.remember("calendarY", contentY)
                Connections { target: backend; function onPresentationChanged() { gridScroll.contentY = backend.presentation.calendarY === undefined ? 7 * calendar.hourHeight - 8 : backend.presentation.calendarY } }
                Item {
                    id: grid
                    x: 62; width: gridScroll.width - 80; height: gridScroll.contentHeight
                    property real dayWidth: width / 7
                    Repeater {
                        model: 7
                        Rectangle {
                            required property int index
                            x: index * grid.dayWidth; width: grid.dayWidth; height: grid.height
                            color: Qt.formatDate(calendar.dayDate(index), "yyyy-MM-dd") === backend.fixtureToday ? Theme.surface : "transparent"
                            Rectangle { width: 1; height: parent.height; color: Theme.line }
                        }
                    }
                    Repeater {
                        model: 24
                        Item {
                            required property int index
                            y: index * calendar.hourHeight; width: grid.width; height: 1
                            Rectangle { anchors.fill: parent; color: Theme.line; opacity: 0.7 }
                            Text { x: -47; y: -7; text: (index < 10 ? "0" : "") + index + ":00"; color: Theme.subtle; font.pixelSize: 10 }
                        }
                    }
                    Repeater {
                        model: backend.events
                        Rectangle {
                            id: eventBox
                            required property var modelData
                            visible: !modelData.all_day && modelData.day_index >= 0 && modelData.day_index < 7
                            x: modelData.day_index * grid.dayWidth + 5 + modelData.column * (grid.dayWidth - 10) / modelData.columns
                            y: modelData.start_hour * calendar.hourHeight
                            width: (grid.dayWidth - 10) / modelData.columns - (modelData.columns > 1 ? 3 : 0)
                            height: Math.max(14, modelData.duration_hours * calendar.hourHeight - 3)
                            clip: true
                            radius: 3; color: eventMouse.containsMouse ? Theme.hover : Theme.selected
                            Rectangle { x: 0; width: 2; height: parent.height; color: Theme.muted }
                            Column {
                                visible: eventBox.height >= 44
                                anchors.fill: parent; anchors.margins: 8; spacing: 4
                                Text { text: eventBox.modelData.time_display; color: Theme.muted; font.pixelSize: 10; textFormat: Text.PlainText }
                                Text { text: eventBox.modelData.title; color: Theme.text; font.pixelSize: 12; font.bold: true; width: parent.width; wrapMode: Text.Wrap; maximumLineCount: Math.max(1, Math.floor((eventBox.height - 24) / 14)); elide: Text.ElideRight; textFormat: Text.PlainText }
                            }
                            Text {
                                visible: eventBox.height < 44
                                anchors.fill: parent; anchors.leftMargin: 7; anchors.rightMargin: 5
                                text: (eventBox.width >= 120 ? eventBox.modelData.time_display + "  " : "") + eventBox.modelData.title
                                textFormat: Text.PlainText; color: Theme.text; font.pixelSize: 10
                                verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight
                            }
                            FmToolTip { visible: eventMouse.containsMouse; text: eventBox.modelData.title + " · " + eventBox.modelData.start_display }
                            MouseArea { id: eventMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: { calendar.selectedEvent = eventBox.modelData; details.open() } }
                            Accessible.role: Accessible.Button
                            Accessible.name: modelData.title + ", " + modelData.start_display
                        }
                    }
                }
            }
        }
    }
    Popup {
        id: details
        anchors.centerIn: parent; width: 400; padding: 24; modal: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; radius: 6; border.color: Theme.line }
        contentItem: ColumnLayout {
            spacing: 16
            Text { text: calendar.selectedEvent.title || ""; color: Theme.text; font.pixelSize: 20; font.bold: true; wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
            Text { text: (calendar.selectedEvent.start_display || "") + " — " + (calendar.selectedEvent.end_display || "") + "\nAmerica/Toronto"; color: Theme.muted; font.pixelSize: 12; lineHeight: 1.5 }
            Text { text: calendar.selectedEvent.location || ""; color: Theme.muted; visible: text.length > 0; textFormat: Text.PlainText }
            Text { text: "Stored fixture occurrence · read-only"; color: Theme.subtle; font.pixelSize: 11 }
            FmButton { text: "Close"; outlined: true; Layout.alignment: Qt.AlignRight; onClicked: details.close() }
        }
    }
}
