// Modal list of events; CalendarView uses it for a day's overflowing all-day
// events ("+N all day"). Set `eventRows` and open(); a row click emits eventSelected.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: eventList
    // Eases in from just under full size; closes at once.
    enter: Transition { ParallelAnimation { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } } }
    parent: Overlay.overlay
    property string title: "Events"
    property var eventRows: []
    property bool available: true
    signal eventSelected(var event)
    anchors.centerIn: parent; width: Math.min(540, parent.width - 40); height: Math.min(480, parent.height - 40); padding: 20; modal: true; focus: true
    onAvailableChanged: if (!available) close()
    background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
    contentItem: ColumnLayout {
        property bool blocksWorkspaceShortcuts: true
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            Text { text: eventList.title; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(18); Layout.fillWidth: true; textFormat: Text.PlainText }
            FmButton { iconName: "close"; implicitWidth: 30; tip: "Close events"; onClicked: eventList.close() }
        }
        ListView {
            id: rows; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; model: eventList.eventRows; ScrollBar.vertical: FmScrollBar {}
            delegate: ItemDelegate {
                required property var modelData; width: rows.width; height: 76
                background: Rectangle { color: parent.hovered ? Theme.hover : "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line } }
                contentItem: Column {
                    spacing: 5
                    Text { text: modelData.title; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText; width: parent.width; elide: Text.ElideRight }
                    Text { text: modelData.start_display + " — " + modelData.end_display; color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); width: parent.width; elide: Text.ElideRight }
                    Text { text: modelData.account_name + " / " + modelData.calendar_name; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); textFormat: Text.PlainText }
                }
                onClicked: { eventList.close(); eventList.eventSelected(modelData) }
            }
        }
    }
}
