import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: history
    objectName: "calendarOperations"
    parent: Overlay.overlay
    readonly property bool live: !backend.syntheticProfile
    property var abandonTarget: ({})
    function accountReady(account) { return backend.connected && (!live || (backend.accounts.length >= 0 && backend.googleSync && backend.accountCapability(account, "calendar_write") && backend.googleSyncState(account).active === true)) }
    function stateLabel(state) { return ({fake_acknowledged: "Saved locally", pending: "Pending", prepared: "Recorded · not sent", ready: "Ready · not sent", in_flight: "In progress", unknown: "Outcome unconfirmed", accepted: "Accepted by Google · refreshing", acknowledged: "Accepted by Google", reconciled: "Observed change · response unconfirmed", conflict: "Event changed on Google", rejected: "Not accepted", abandoned: "Abandoned"})[state] || "Needs attention" }
    function actionFor(state) { return ["prepared", "ready"].indexOf(state) >= 0 ? "resume" : ["unknown", "accepted"].indexOf(state) >= 0 ? "reconcile" : "check" }
    function occurrenceLabel(value) { return value && value.original_start ? "This occurrence · originally " + (value.original_start.date || value.original_start.instant || "") : "" }
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
            Text { text: history.live ? "Calendar activity" : "Local calendar activity"; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(20); font.weight: Font.Medium; Layout.fillWidth: true }
            FmButton { iconName: "refresh"; implicitWidth: 32; tip: "Refresh activity"; enabled: !backend.calendarOperationsLoading; onClicked: backend.loadCalendarOperations() }
            FmButton { iconName: "close"; implicitWidth: 32; tip: "Close activity"; onClicked: history.close() }
        }
        Text { text: history.live ? "Resume sends a recorded, unsent change. Check outcome only reads Google. Google may notify guests or the organizer; notification delivery is unverified." : "Pending operations appear first and can be retried after a restart without creating another event. Load more to reach older activity. No invitations are sent."; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true }
        ListView {
            id: operationList; objectName: "calendarOperationList"; Layout.fillWidth: true; Layout.fillHeight: true; clip: true; spacing: 6; model: backend.calendarOperations
            ScrollBar.vertical: ScrollBar { }
            delegate: Rectangle {
                id: operationRow
                required property var modelData; width: operationList.width; height: operationContent.implicitHeight + 20; color: Theme.canvas; radius: 5; border.color: Theme.line
                RowLayout {
                    id: operationContent; anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 10; spacing: 12
                    ColumnLayout {
                        Layout.fillWidth: true; spacing: 5
                        Text { text: (modelData.kind === "rsvp" ? "RSVP" : modelData.kind === "create" ? "Create event" : modelData.kind === "delete" ? "Delete event" : "Edit event") + " · " + history.stateLabel(modelData.state); color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); wrapMode: Text.Wrap; Layout.fillWidth: true }
                        Text { text: modelData.account_name + " / " + modelData.target.calendar; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; elide: Text.ElideMiddle; Layout.fillWidth: true }
                        Text { text: modelData.request_id; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(10); textFormat: Text.PlainText; Layout.fillWidth: true; elide: Text.ElideMiddle }
                        Text { objectName: "calendarOperationOccurrence-" + operationRow.modelData.request_id; visible: !!modelData.occurrence; text: history.occurrenceLabel(modelData.occurrence); color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
                        Text { objectName: "calendarOperationEvidence-" + operationRow.modelData.request_id; visible: history.live && ["accepted", "acknowledged", "reconciled"].indexOf(modelData.state) >= 0; text: modelData.state === "reconciled" ? "Observed event matches the recorded change. The original response and notification delivery remain unconfirmed; this change will not be sent again." : "Provider response retained. Notification delivery is unverified."; color: Theme.subtle; font.pixelSize: Theme.fontSize(11); wrapMode: Text.Wrap; Layout.fillWidth: true }
                        Text { visible: !!modelData.error; text: modelData.error ? modelData.error.message : ""; color: Theme.error; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true }
                    }
                    ColumnLayout {
                        spacing: 4
                        FmButton {
                            objectName: "calendarOperationResolve-" + operationRow.modelData.request_id
                            visible: ["fake_acknowledged", "acknowledged", "reconciled", "abandoned"].indexOf(modelData.state) < 0
                            text: modelData.account_archived ? "Check saved status" : !history.live ? "Retry" : history.actionFor(modelData.state) === "resume" ? "Resume" : modelData.state === "accepted" ? "Refresh" : history.actionFor(modelData.state) === "reconcile" ? "Check outcome" : "Check status"; outlined: true
                            enabled: !backend.calendarOperationsLoading && backend.connected && (modelData.account_archived || !history.live || history.actionFor(modelData.state) === "check" || history.accountReady(modelData.account))
                            onClicked: { if (history.live) backend.calendarOperationAction(modelData.account, modelData.request_id, modelData.account_archived ? "check" : history.actionFor(modelData.state), modelData.revision); else backend.retryCalendarOperation(modelData.account, modelData.request_id) }
                        }
                        FmButton {
                            objectName: "calendarOperationAbandon-" + operationRow.modelData.request_id
                            visible: !modelData.account_archived && history.live && modelData.revision >= 0 && ["prepared", "ready", "unknown", "conflict", "rejected"].indexOf(modelData.state) >= 0; text: "Abandon"
                            enabled: !backend.calendarOperationsLoading && backend.connected
                            onClicked: { history.abandonTarget = {account: modelData.account, request_id: modelData.request_id, revision: modelData.revision}; abandonConfirmation.open() }
                        }
                    }
                }
            }
            Text { anchors.centerIn: parent; visible: operationList.count === 0; text: backend.calendarOperationsLoading ? "Loading activity…" : history.live ? "No calendar changes yet." : "No local calendar changes yet."; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
        }
        Text { visible: backend.calendarOperationStatus.length > 0; text: backend.calendarOperationStatus; color: Theme.accent; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); wrapMode: Text.Wrap; Layout.fillWidth: true; textFormat: Text.PlainText }
        FmButton { visible: backend.calendarOperationsHasMore; text: "Load more activity"; outlined: true; enabled: !backend.calendarOperationsLoading && backend.connected; Layout.alignment: Qt.AlignHCenter; onClicked: backend.loadMoreCalendarOperations() }
    }
    Dialog {
        id: abandonConfirmation; objectName: "calendarOperationAbandonConfirmation"; parent: Overlay.overlay; anchors.centerIn: parent; modal: true
        implicitWidth: 324; width: Math.min(implicitWidth, parent.width - 32)
        implicitHeight: topPadding + bottomPadding + abandonBody.implicitHeight + (header ? header.implicitHeight + spacing : 0) + (footer ? footer.implicitHeight + spacing : 0)
        title: "Abandon recorded change?"; standardButtons: Dialog.Ok | Dialog.Cancel
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        palette.windowText: Theme.text; palette.buttonText: Theme.text
        onOpened: standardButton(Dialog.Ok).text = "Abandon"
        contentItem: Item {
            implicitHeight: abandonBody.implicitHeight
            Text { id: abandonBody; width: abandonConfirmation.availableWidth; text: "Stop working on this recorded change. This does not undo a change already accepted by Google."; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); wrapMode: Text.Wrap; textFormat: Text.PlainText }
        }
        onAccepted: backend.calendarOperationAction(history.abandonTarget.account, history.abandonTarget.request_id, "abandon", history.abandonTarget.revision)
    }
}
