import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: eventList
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
            Text { text: eventList.title; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(19); Layout.fillWidth: true; textFormat: Text.PlainText }
            FmButton { iconName: "close"; implicitWidth: 30; tip: "Close events"; onClicked: eventList.close() }
        }
        ListView {
            id: rows; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; model: eventList.eventRows; ScrollBar.vertical: ScrollBar { }
            delegate: ItemDelegate {
                required property var modelData; width: rows.width; height: 76
                background: Rectangle { color: parent.hovered ? Theme.hover : "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line } }
                contentItem: Column {
                    spacing: 5
                    Text { text: modelData.title; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); textFormat: Text.PlainText; width: parent.width; elide: Text.ElideRight }
                    Text { text: modelData.start_display + " — " + modelData.end_display; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); width: parent.width; elide: Text.ElideRight }
                    Text { text: modelData.account_name + " / " + modelData.calendar_name; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(10); textFormat: Text.PlainText }
                }
                onClicked: { eventList.close(); eventList.eventSelected(modelData) }
            }
        }
    }
}
