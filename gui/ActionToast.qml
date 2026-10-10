// The transient toast at the bottom of the main window (send progress, Undo
// after an action, and so on). Text, optional action button and progress or
// success icon all come from `backend.actionToast`; Main.qml positions it.
// It rises into place and sinks as it fades; an Undo toast draws a line along
// its foot that runs out with the time left to undo.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Rectangle {
    id: toast; objectName: "actionToast"
    property real maximumWidth: parent ? parent.width * 0.7 : 600
    readonly property bool shown: !!backend.actionToast.visible
    readonly property bool undoable: shown && String(backend.actionToast.action || "").indexOf("undo_") === 0
    // A new toast, not the same one changing its text.
    readonly property string identity: shown ? String(backend.actionToast.issued_at) + "|" + (backend.actionToast.action || "") : ""
    visible: shown || opacity > 0
    opacity: shown ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.ms(toast.shown ? Theme.baseMs : Theme.quickMs); easing.type: toast.shown ? Theme.enterEasing : Theme.exitEasing } }
    property real rise: shown ? 0 : 8
    Behavior on rise { NumberAnimation { duration: Theme.ms(toast.shown ? Theme.baseMs : Theme.quickMs); easing.type: toast.shown ? Theme.enterEasing : Theme.exitEasing } }
    transform: Translate { y: toast.rise }
    onIdentityChanged: countdown.restart()
    width: Math.min(maximumWidth, message.implicitWidth + action.implicitWidth + (action.visible ? 14 : 0) + (feedbackIcon.visible ? 28 : 0) + 32)
    height: Math.ceil(40 * Theme.fontScale)
    color: Theme.bg4; border.color: Theme.line2; radius: 8
    Accessible.role: Accessible.StaticText
    Accessible.name: backend.actionToast.text || ""
    layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
    layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.55); shadowBlur: .8; shadowVerticalOffset: 18 }
    Rectangle {
        id: countdown; objectName: "actionToastCountdown"
        property real remaining: 0
        function restart() {
            drain.stop()
            const total = Number(backend.actionToast.duration) || 0, timeLeft = Number(backend.actionToast.remaining_ms) || 0
            if (!toast.undoable || total <= 0 || timeLeft <= 0) { remaining = 0; return }
            remaining = Math.min(1, timeLeft / total)
            drain.from = remaining; drain.duration = timeLeft; drain.start()
        }
        visible: toast.undoable && !Theme.reducedMotion && remaining > 0
        x: 8; y: parent.height - 3; width: Math.max(0, (parent.width - 16) * remaining); height: 2; radius: 1
        color: Theme.accentAlpha(.5)
        // Time runs at an even pace.
        NumberAnimation on remaining { id: drain; running: false; to: 0; easing.type: Easing.Linear }
    }
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
            color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        }
        Button {
            id: action; objectName: "actionToastButton"
            property bool blocksWorkspaceShortcuts: true
            visible: (backend.actionToast.action_label || "").length > 0
            implicitWidth: visible ? contentItem.implicitWidth : 0
            implicitHeight: toast.height - 8; padding: 0
            text: backend.actionToast.action_label || ""
            Accessible.name: text + ": " + (backend.actionToast.text || "")
            contentItem: RowLayout {
                spacing:7
                Text {text:action.text;color:action.hovered ? Theme.text : Theme.accent;font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(12);font.weight:Font.Medium}
                Rectangle {
                    visible:action.text==="Undo"; implicitWidth:undoKey.implicitWidth+10; implicitHeight:18; radius:3; color:"transparent"; border.color:Theme.line2
                    Text {id:undoKey;anchors.centerIn:parent;text:ShortcutRegistry.label(ShortcutRegistry.sequence("undo",backend.preferences));color:Theme.inkAlpha(.45);font.family:Theme.fontFamily;font.pointSize:Theme.fontPoints(10.5)}
                }
            }
            background: Rectangle { color: "transparent"; radius: 5; border.color: action.visualFocus ? Theme.accent : "transparent" }
            onClicked: backend.activateActionToast()
        }
    }
}
