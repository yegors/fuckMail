import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: history
    parent: Overlay.overlay
    width: Math.min(640, parent.width - 36); height: Math.min(530, parent.height - 36); padding: 24
    anchors.centerIn: parent; modal: true; focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle { color: Theme.surface; radius: 8; border.color: Theme.controlBorder }
    onOpened: backend.loadCalendarOperations()
    contentItem: ColumnLayout {
        property bool blocksWorkspaceShortcuts: true
        spacing: 14
        RowLayout {
            Layout.fillWidth: true
            Text { text: "Local calendar activity"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(20); font.weight: Font.Medium; Layout.fillWidth: true }
            FmButton { iconName: "refresh"; implicitWidth: 32; tip: "Refresh local activity"; enabled: !backend.calendarOperationsLoading; onClicked: backend.loadCalendarOperations() }
            FmButton { iconName: "close"; implicitWidth: 32; tip: "Close activity"; onClicked: history.close() }
        }
        Text { text: "Pending operations appear first and can be retried after a restart without creating another event. Load more to reach older activity. No invitations are sent."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
        ListView {
            id: operationList; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; spacing: 6; model: backend.calendarOperations
            ScrollBar.vertical: ScrollBar { }
            delegate: Rectangle {
                required property var modelData; width: operationList.width; height: operationContent.implicitHeight + 20; color: Theme.canvas; radius: 5; border.color: Theme.line
                RowLayout {
                    id: operationContent; anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 10; spacing: 12
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 5
                        Text { text: (modelData.kind === "rsvp" ? "RSVP" : modelData.kind === "create" ? "Create event" : "Edit event") + " · " + (modelData.state === "fake_acknowledged" ? "Saved locally" : modelData.state === "pending" ? "Pending" : "Needs attention"); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
                        Text { text: modelData.account_name + " / " + modelData.target.calendar; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText }
                        Text { text: modelData.request_id; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(10); textFormat: Text.PlainText; Layout.fillWidth: true; elide: Text.ElideMiddle }
                        Text { visible: !!modelData.error; text: modelData.error ? modelData.error.message : ""; color: Theme.error; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
                    }
                    FmButton { visible: modelData.state !== "fake_acknowledged"; text: "Retry"; outlined: true; enabled: !backend.calendarOperationsLoading && backend.connected; onClicked: backend.retryCalendarOperation(modelData.account, modelData.request_id) }
                }
            }
            Text { anchors.centerIn: parent; visible: operationList.count === 0; text: backend.calendarOperationsLoading ? "Loading local activity…" : "No local calendar changes yet."; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
        }
        Text { visible: backend.calendarOperationStatus.length > 0; text: backend.calendarOperationStatus; color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
        FmButton { visible: backend.calendarOperationsHasMore; text: "Load more activity"; outlined: true; enabled: !backend.calendarOperationsLoading && backend.connected; Layout.alignment: Qt.AlignHCenter; onClicked: backend.loadMoreCalendarOperations() }
    }
}
