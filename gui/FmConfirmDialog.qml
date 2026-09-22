import QtQuick
import QtQuick.Controls

FmDialog {
    id: dialog
    property string message: ""
    property string messageObjectName: ""
    property string actionText: "Continue"
    property string actionObjectName: ""
    property string cancelObjectName: ""
    property bool actionEnabled: true
    standardButtons: Dialog.Cancel | Dialog.Ok
    onAboutToShow: {
        const action = standardButton(Dialog.Ok)
        action.text = actionText
        action.objectName = actionObjectName
        action.enabled = actionEnabled
        standardButton(Dialog.Cancel).objectName = cancelObjectName
    }
    contentItem: Label {
        objectName: dialog.messageObjectName
        width: dialog.availableWidth
        property bool blocksWorkspaceShortcuts: true
        text: dialog.message
        color: Theme.inkAlpha(0.6)
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSize(13)
        wrapMode: Text.Wrap
        textFormat: Text.PlainText
        lineHeight: 1.5
    }
    onActionEnabledChanged: { const action = standardButton(Dialog.Ok); if (action) action.enabled = actionEnabled }
    Shortcut {
        sequences: ["Return", "Enter"]
        enabled: dialog.opened && dialog.actionEnabled
        onActivated: dialog.accept()
    }
}
