import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// An opened Spam message: its headers from the listing and its text read
// live from Gmail. Links and images are not loaded.
Rectangle {
    id: pane
    objectName: "spamReader"
    required property var spam
    property int margin: 40
    color: Theme.bg0
    readonly property var row: spam.selectedRow
    Text {
        anchors.centerIn: parent; visible: !pane.spam.selected.length
        text: "Choose a message to read it."
        color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13)
    }
    ScrollView {
        anchors.fill: parent; visible: pane.spam.selected.length > 0
        contentWidth: availableWidth
        ScrollBar.vertical: FmScrollBar {}
        ColumnLayout {
            width: parent.width; spacing: 14
            Item { implicitHeight: 10 }
            Text {
                objectName: "spamSubject"
                Layout.fillWidth: true; Layout.leftMargin: pane.margin; Layout.rightMargin: pane.margin
                text: pane.row.subject || "(No subject)"; textFormat: Text.PlainText; wrapMode: Text.Wrap
                color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(24); font.weight: Font.Medium
            }
            Text {
                Layout.fillWidth: true; Layout.leftMargin: pane.margin; Layout.rightMargin: pane.margin
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                text: (pane.row.sender_name ? pane.row.sender_name + " <" + pane.row.sender_email + ">" : pane.row.sender_email || "")
                    + (pane.row.internal_date_ms ? " · " + Qt.formatDateTime(new Date(pane.row.internal_date_ms), "MMM d, yyyy HH:mm") : "")
                color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13)
            }
            Rectangle { Layout.fillWidth: true; Layout.leftMargin: pane.margin; Layout.rightMargin: pane.margin; implicitHeight: 1; color: Theme.line }
            Text {
                objectName: "spamReaderStatus"
                Layout.fillWidth: true; Layout.leftMargin: pane.margin; Layout.rightMargin: pane.margin
                visible: text.length > 0; wrapMode: Text.Wrap
                text: pane.spam.opening ? "Reading from Gmail…" : pane.spam.openError
                color: pane.spam.openError ? Theme.error : Theme.subtle
                font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
            }
            TextEdit {
                objectName: "spamBody"
                Layout.fillWidth: true; Layout.leftMargin: pane.margin; Layout.rightMargin: pane.margin
                visible: !pane.spam.opening && !pane.spam.openError.length
                readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap
                text: pane.spam.text; color: Theme.text; selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
                font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(14)
            }
            Item { implicitHeight: 32 }
        }
    }
}
