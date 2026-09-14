import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    objectName: "calendarSearchPopup"
    property bool combined: true
    signal eventRequested(var event)
    readonly property var result: backend.calendarSearch
    readonly property var rows: result.events || []
    function lastDate(value) {
        if (!value) return ""
        const date = new Date(value + "T12:00:00")
        date.setDate(date.getDate() - 1)
        return Qt.formatDate(date, "yyyy-MM-dd")
    }
    width: Math.min(440, parent ? parent.width - 24 : 440)
    height: Math.min(contents.implicitHeight + 32, parent ? parent.height - y - 12 : 520)
    padding: 16
    focus: false
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
    contentItem: ColumnLayout {
        id: contents
        spacing: 10
        RowLayout {
            Layout.fillWidth: true
            Text {
                Layout.fillWidth: true
                text: popup.combined ? "Search selected calendars" : "Search " + (backend.currentAccount.name || "this account")
                elide: Text.ElideRight; textFormat: Text.PlainText
                color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13); weight: Font.Medium }
            }
            FmButton { objectName: "calendarSearchClose"; text: "Esc"; implicitHeight: 21; implicitWidth: 31; leftPadding: 4; rightPadding: 4; font.pixelSize: Theme.fontSize(11); outlined: true; onClicked: popup.close() }
        }
        Text {
            objectName: "calendarSearchRange"
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: (popup.result.from || "") + " – " + popup.lastDate(popup.result.to)
                + " · " + (popup.result.time_zone || backend.preferences.calendar_display_time_zone || "display time zone")
            color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
        }
        Text {
            objectName: "calendarSearchStatus"
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: popup.result.status || "Enter words to search this date range"
            color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
        }
        ListView {
            id: results
            objectName: "calendarSearchResults"
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(contentHeight, 280)
            Layout.fillHeight: true
            clip: true; spacing: 4; model: popup.rows
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            delegate: FmButton {
                id: resultButton
                required property var modelData
                required property int index
                objectName: "calendarSearchResult_" + index
                width: results.width; height: label.implicitHeight + 20
                enabled: !popup.result.busy
                leftPadding: 10; rightPadding: 10
                text: modelData.title || "Untitled event"
                tip: (modelData.calendar_name || modelData.calendar_id || "") + " · " + (modelData.account || "")
                onClicked: popup.eventRequested(modelData)
                contentItem: Column {
                    id: label; spacing: 4
                    Text {
                        width: parent.width; text: resultButton.text; textFormat: Text.PlainText; elide: Text.ElideRight
                        color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(13); weight: Font.Medium }
                    }
                    Text {
                        width: parent.width; textFormat: Text.PlainText; elide: Text.ElideRight
                        text: (resultButton.modelData.date || "") + " · " + (resultButton.modelData.time_display || "")
                        color: Theme.secondary; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
                    }
                    Text {
                        width: parent.width; textFormat: Text.PlainText; elide: Text.ElideRight
                        text: resultButton.modelData.calendar_name || resultButton.modelData.calendar_id || ""
                        color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
                    }
                }
            }
        }
        Text {
            Layout.fillWidth: true; visible: !popup.result.busy && !!popup.result.events && !popup.rows.length
            text: "No matching saved events in this date range."
            wrapMode: Text.Wrap; color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
        }
        Text {
            Layout.fillWidth: true; visible: popup.result.has_more === true
            text: "More matches are available. Narrow the words or date range."
            wrapMode: Text.Wrap; color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
        }
    }
}
