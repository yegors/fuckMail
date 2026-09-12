import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: field
    required property string label
    required property string fieldName
    property bool showLinks: false
    signal ccRequested()
    signal bccRequested()
    readonly property var recipients: backend.draft[fieldName] || []
    property string identity: backend.account + "/" + (backend.activeDraftId || backend.selectedId)
    implicitHeight: Math.max(24, chips.implicitHeight) + 14
    function commit() {
        if (!entry.text.trim().length) return true
        if (!backend.addDraftRecipient(fieldName, entry.text)) return false
        entry.text = ""; return true
    }
    onIdentityChanged: entry.text = ""
    Connections {
        target: backend
        function onDraftChanged() {
            const text = backend.recipientInputs[field.fieldName] || ""
            if (entry.text !== text) entry.text = text
        }
    }
    Rectangle { y: Theme.snapY(parent, parent.height-1); width: parent.width; height: Theme.ruleWidth; color: Theme.line }
    Text { x: 0; y: 10; width: 44; text: field.label; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); color: Theme.subtle }
    Flow {
        id: chips
        x: 54; y: 7; width: Math.max(0, parent.width-54-(field.showLinks ? links.width+12 : 0)); spacing: 4
        Repeater {
            model: field.recipients
            Rectangle {
                required property var modelData
                required property int index
                height: 24; width: Math.min(recipientName.implicitWidth+31,chips.width); radius: 12; color: "transparent"; border.color: Theme.controlBorder
                Text { id: recipientName; anchors.left: parent.left; anchors.leftMargin: 9; anchors.right: removeRecipient.left; anchors.rightMargin: 3; anchors.verticalCenter: parent.verticalCenter; text: modelData.name || modelData.email; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); elide: Text.ElideRight; textFormat: Text.PlainText }
                FmButton { id: removeRecipient; anchors.right: parent.right; anchors.rightMargin: 3; anchors.verticalCenter: parent.verticalCenter; implicitWidth: 16; implicitHeight: 18; leftPadding: 2; rightPadding: 2; iconName: "close"; iconSize: 9; tip: "Remove "+modelData.email; enabled: !backend.draftLocked; onClicked: backend.removeDraftRecipient(field.fieldName,index) }
            }
        }
        TextField {
            id: entry
            objectName: "draftRecipient_"+field.fieldName
            property bool blocksWorkspaceShortcuts: true
            width: Math.min(160,chips.width); height: 24; padding: 0; background: Item {}
            color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); placeholderTextColor: Theme.subtle
            placeholderText: field.recipients.length ? "" : "Add recipient"
            enabled: !backend.draftLocked; selectByMouse: true
            onTextEdited: backend.stageDraftRecipient(field.fieldName,text)
            onAccepted: field.commit()
            onEditingFinished: if (text.trim().length) field.commit()
            Accessible.name: field.label+" recipient"
        }
    }
    Row {
        id: links
        visible: field.showLinks; anchors.right: parent.right; y: 7; spacing: 10
        FmButton { objectName: "draftShowCc"; text: "Cc"; implicitHeight: 24; leftPadding: 0; rightPadding: 0; font.pixelSize: Theme.fontSize(12); onClicked: field.ccRequested() }
        FmButton { objectName: "draftShowBcc"; text: "Bcc"; implicitHeight: 24; leftPadding: 0; rightPadding: 0; font.pixelSize: Theme.fontSize(12); onClicked: field.bccRequested() }
    }
}
