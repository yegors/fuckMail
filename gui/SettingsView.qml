import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: settings
    signal done()
    property string page: "Mailboxes & calendars"
    RowLayout {
        anchors.fill: parent; spacing: 0
        Rectangle {
            Layout.fillHeight: true; Layout.preferredWidth: 205; color: Theme.canvas
            Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.line }
            ColumnLayout {
                anchors.fill: parent; anchors.margins: 16; spacing: 5
                Text { text: "Settings"; color: Theme.text; font.pixelSize: 20; font.bold: true; Layout.topMargin: 7; Layout.bottomMargin: 16 }
                Repeater {
                    model: ["Mailboxes & calendars", "Local model", "Mail", "Calendar", "Shortcuts"]
                    FmButton { required property string modelData; text: modelData; textAlignment: Text.AlignLeft; Layout.fillWidth: true; selected: settings.page === modelData; onClicked: settings.page = modelData }
                }
                Item { Layout.fillHeight: true }
                Text { text: "Phase 0 · synthetic profile\nEverything shown stays on this machine."; color: Theme.subtle; font.pixelSize: 10; wrapMode: Text.Wrap; Layout.fillWidth: true; lineHeight: 1.4 }
            }
        }
        Item {
            Layout.fillWidth: true; Layout.fillHeight: true
            FmButton { anchors.top: parent.top; anchors.right: parent.right; anchors.margins: 20; text: "Done  Esc"; outlined: true; onClicked: settings.done() }
            ColumnLayout {
                anchors.top: parent.top; anchors.left: parent.left; anchors.margins: 32; width: Math.min(570, parent.width - 150); spacing: 20
                Text { text: settings.page; color: Theme.text; font.pixelSize: 21; font.bold: true }
                Text { text: "Settings navigation is ready. Detailed controls will follow the expanded design."; color: Theme.muted; font.pixelSize: 12; wrapMode: Text.Wrap; Layout.fillWidth: true; lineHeight: 1.4 }
                Repeater {
                    model: settings.page === "Mailboxes & calendars" ? backend.accounts : []
                    Rectangle {
                        required property var modelData
                        Layout.fillWidth: true; implicitHeight: 98; color: "transparent"; border.color: Theme.line; radius: 6
                        ColumnLayout {
                            anchors.fill: parent; anchors.margins: 18; spacing: 6
                            Text { text: modelData.name; color: Theme.text; font.pixelSize: 13; font.bold: true; textFormat: Text.PlainText }
                            Text { text: modelData.email; color: Theme.muted; font.pixelSize: 11; textFormat: Text.PlainText }
                            Text { text: "Synthetic Google-shaped account · " + modelData.conversation_count + " conversations"; color: Theme.subtle; font.pixelSize: 10 }
                        }
                    }
                }
                Text { visible: settings.page !== "Mailboxes & calendars"; text: "No configuration changes are made on this page yet."; color: Theme.subtle; font.pixelSize: 11; wrapMode: Text.Wrap; Layout.fillWidth: true }
            }
        }
    }
}
