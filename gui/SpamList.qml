import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The Spam view's list: sender, date, subject and Google's snippet, fetched
// live from Gmail 50 at a time.
ListView {
    id: view
    objectName: "spamList"
    required property var spam
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    model: spam.rows
    ScrollBar.vertical: FmScrollBar {}
    function when(ms) {
        if (!ms) return ""
        const at = new Date(ms), now = new Date()
        if (at.toDateString() === now.toDateString()) return Qt.formatTime(at, "HH:mm")
        return Qt.formatDate(at, at.getFullYear() === now.getFullYear() ? "MMM d" : "MMM d, yyyy")
    }
    header: Text {
        width: view.width; leftPadding: 24; rightPadding: 24; topPadding: 4; bottomPadding: 10
        wrapMode: Text.Wrap
        text: "Read live from Gmail each time you open it. Spam is not downloaded or kept on this computer."
        color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
    }
    delegate: ItemDelegate {
        id: row
        required property var modelData
        objectName: "spamRow_" + modelData.id
        width: view.width
        leftPadding: 24; rightPadding: 24; topPadding: 12; bottomPadding: 12
        readonly property bool current: view.spam.selected === modelData.id
        background: Rectangle { color: row.current ? Theme.selected : row.hovered ? Theme.hover : "transparent" }
        onClicked: view.spam.open(modelData.id)
        contentItem: ColumnLayout {
            spacing: 4
            RowLayout {
                spacing: 12
                Text {
                    Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                    text: row.modelData.sender_name || row.modelData.sender_email || "(Unknown sender)"
                    color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(14)
                    font.weight: row.modelData.unread ? Font.DemiBold : Font.Normal
                }
                Text {
                    text: view.when(row.modelData.internal_date_ms)
                    color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); font.features: {"tnum": 1}
                }
            }
            Text {
                Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                text: row.modelData.subject || "(No subject)"
                color: Theme.secondary; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13.5)
            }
            Text {
                Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText; maximumLineCount: 1
                visible: text.length > 0; text: row.modelData.snippet || ""
                color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
            }
        }
    }
    footer: ColumnLayout {
        width: view.width; spacing: 10
        Item { implicitHeight: 6 }
        Text {
            objectName: "spamStatus"
            Layout.fillWidth: true; Layout.leftMargin: 24; Layout.rightMargin: 24
            visible: text.length > 0; wrapMode: Text.Wrap; horizontalAlignment: Text.AlignHCenter
            text: view.spam.error ? view.spam.error
                : view.spam.loading ? "Reading Spam from Gmail…"
                : !view.spam.rows.length ? "Spam is empty." : ""
            color: view.spam.error ? Theme.error : Theme.subtle
            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5)
        }
        FmButton {
            objectName: "spamMore"; Layout.alignment: Qt.AlignHCenter
            visible: !view.spam.loading && (view.spam.error.length > 0 || view.spam.nextToken.length > 0)
            text: view.spam.error ? "Try again" : "Load more"; outlined: true
            onClicked: view.spam.more()
        }
        Item { implicitHeight: 18 }
    }
}
