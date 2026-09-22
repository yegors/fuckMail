import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: panel
    width: 390
    height: Math.min(370, cards.contentHeight + 24)
    color: Theme.canvas; border.color: Theme.line; radius: 8
    visible: backend.reminders.length > 0
    ListView {
        id: cards
        anchors.fill: parent; anchors.margins: 12
        spacing: 14; clip: true
        model: backend.reminders
        ScrollBar.vertical: FmScrollBar {}
        delegate: ColumnLayout {
            required property var modelData
            width: cards.width - 8; spacing: 5
            Text { text: modelData.timing || "Soon"; color: Theme.accent; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText }
            Text { text: modelData.title || "Event"; color: Theme.text; font.pixelSize: Theme.fontSize(14); font.weight: Font.Medium; textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
            Text { text: (modelData.account_name || "") + " · " + (modelData.calendar_name || ""); color: Theme.muted; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                FmButton { text: "Dismiss"; implicitHeight: 28; onClicked: backend.dismissCalendarReminder(modelData) }
                FmButton { text: "Join"; visible: !!modelData.conference_url; prominent: true; implicitHeight: 28; onClicked: backend.joinCalendarEvent(modelData) }
            }
        }
    }
}
