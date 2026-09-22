import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Rectangle {
    id: toast; objectName: "actionToast"
    property real maximumWidth: parent ? parent.width * 0.7 : 600
    visible: opacity > 0
    opacity: backend.actionToast.visible ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: 120 } }
    width: Math.min(maximumWidth, message.implicitWidth + action.implicitWidth + (action.visible ? 14 : 0) + (feedbackIcon.visible ? 28 : 0) + 32)
    height: Math.ceil(40 * Theme.fontScale)
    color: Theme.bg4; border.color: Theme.line2; radius: 6
    Accessible.role: Accessible.StaticText
    Accessible.name: backend.actionToast.text || ""
    layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
    layer.effect: MultiEffect { shadowEnabled: true; shadowColor: "#80000000"; shadowVerticalOffset: 12; shadowBlur: 0.7 }
    RowLayout {
        anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 14
        FmIcon {
            id: feedbackIcon; objectName: "sendProgressIcon"
            visible: backend.actionToast.progress || backend.actionToast.success || false
            name: backend.actionToast.progress ? "refresh" : "check"; size: 14; color: Theme.accent
            RotationAnimator on rotation { from: 0; to: 360; duration: 900; loops: Animation.Infinite; running: toast.visible && !!backend.actionToast.progress }
            onNameChanged: if (name === "check") rotation = 0
        }
        Text {
            id: message; Layout.fillWidth: true
            text: backend.actionToast.text || ""; textFormat: Text.PlainText; elide: Text.ElideRight
            color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13)
        }
        Button {
            id: action; objectName: "actionToastButton"
            property bool blocksWorkspaceShortcuts: true
            visible: (backend.actionToast.action_label || "").length > 0
            implicitWidth: visible ? contentItem.implicitWidth : 0
            implicitHeight: toast.height - 8; padding: 0
            text: backend.actionToast.action_label || ""
            Accessible.name: text + ": " + (backend.actionToast.text || "")
            contentItem: Text { text: action.text; color: action.hovered ? Theme.text : Theme.accent; verticalAlignment: Text.AlignVCenter; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13); font.weight: Font.Medium }
            background: Rectangle { color: "transparent"; radius: 3; border.color: action.activeFocus ? Theme.accent : "transparent" }
            onClicked: backend.activateActionToast()
        }
    }
}
