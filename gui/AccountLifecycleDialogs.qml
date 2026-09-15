import QtQuick
import QtQuick.Controls

Item {
    id: lifecycle
    readonly property bool popupOpen: removeDialog.opened || bufferDialog.opened
    Connections {
        target: backend
        function onAccountRemovalReady(context) { removeDialog.context = context; removeDialog.open() }
        function onAccountBufferDispositionRequired() { bufferDialog.open() }
    }
    FmConfirmDialog {
        id: removeDialog; objectName: "accountRemoveDialog"
        property var context: ({})
        title: "Remove account?"
        message: (context.email || "") + " will be removed from this app. Saved mail, drafts, files and history stay in a read-only archive; local sign-in credentials are removed. Nothing changes at Google."
        destructive: true
        actionText: "Remove"
        actionObjectName: "confirmAccountRemoval"
        cancelObjectName: "cancelAccountRemoval"
        onAccepted: backend.confirmAccountRemoval(context)
    }
    FmConfirmDialog {
        id: bufferDialog; objectName: "removedAccountBufferDialog"
        closePolicy: Popup.NoAutoClose
        title: "Discard unsaved edits?"
        message: "These edits belong to the removed account " + (backend.accountLifecycle.unsaved_email || "") + ". They have not been saved or moved to another account. Cancel to keep and copy them, or discard only the unsaved edits. The saved draft remains in the archive."
        destructive: true
        actionText: "Discard edits"
        actionObjectName: "discardRemovedAccountBuffer"
        cancelObjectName: "keepRemovedAccountBuffer"
        onAccepted: backend.resolveUnsavedAccountBuffer(true)
        onRejected: backend.resolveUnsavedAccountBuffer(false)
    }
}
