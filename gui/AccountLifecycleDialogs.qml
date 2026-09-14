import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: lifecycle
    readonly property bool popupOpen: removeDialog.opened || bufferDialog.opened
    Connections {
        target: backend
        function onAccountRemovalReady(context) { removeDialog.context = context; removeDialog.open() }
        function onAccountBufferDispositionRequired() { bufferDialog.open() }
    }
    Dialog {
        id: removeDialog; objectName: "accountRemoveDialog"
        property var context: ({})
        parent: Overlay.overlay; anchors.centerIn: parent; modal: true
        width: Math.min(500, parent ? parent.width - 40 : 500); padding: 24
        closePolicy: Popup.CloseOnEscape
        background: Rectangle { color: Theme.canvas; radius: 10; border.color: Theme.line }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 16
            Text { text: "Remove account from this app?"; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(18); font.weight: Font.Medium }
            Text { objectName: "accountRemovalIdentity"; text: removeDialog.context.email || ""; Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14) }
            Text { text: "Saved mail, drafts, files and history will remain in a read-only archive. Local sign-in credentials will be removed. Mail and events at Google are unchanged.\n\nTo use this account again, sign in as the same Google account. Restoring it will not start sync or send mail."; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            Flow {
                Layout.fillWidth: true; spacing: 8
                FmButton { objectName: "cancelAccountRemoval"; text: "Cancel"; onClicked: removeDialog.close() }
                FmButton { objectName: "confirmAccountRemoval"; text: "Remove · keep archive"; outlined: true; onClicked: { const context = removeDialog.context; removeDialog.close(); backend.confirmAccountRemoval(context) } }
            }
        }
    }
    Dialog {
        id: bufferDialog; objectName: "removedAccountBufferDialog"
        parent: Overlay.overlay; anchors.centerIn: parent; modal: true
        width: Math.min(500, parent ? parent.width - 40 : 500); padding: 24
        closePolicy: Popup.NoAutoClose
        background: Rectangle { color: Theme.canvas; radius: 10; border.color: Theme.line }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 16
            Text { text: "Unsaved edits belong to a removed account"; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(18); font.weight: Font.Medium }
            Text { text: backend.accountLifecycle.unsaved_email || ""; Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14) }
            Text { text: "The editor still holds your unsaved text and recipient inputs for this account. They have not been moved to another account or saved into the archive. Keep this editor open to select and copy them, or explicitly discard only these unsaved edits to continue. The saved draft and receipts stay in the archive."; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            Flow {
                Layout.fillWidth: true; spacing: 8
                FmButton { objectName: "keepRemovedAccountBuffer"; text: "Keep here"; outlined: true; onClicked: { bufferDialog.close(); backend.resolveUnsavedAccountBuffer(false) } }
                FmButton { objectName: "discardRemovedAccountBuffer"; text: "Discard unsaved edits"; onClicked: { bufferDialog.close(); backend.resolveUnsavedAccountBuffer(true) } }
            }
        }
    }
}
