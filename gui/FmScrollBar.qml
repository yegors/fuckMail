// The app's themed ScrollBar, attached to ScrollViews and ListViews
// throughout the UI.
import QtQuick
import QtQuick.Controls

ScrollBar {
    id: bar
    objectName: "persistentScrollBar"
    // Everywhere the control stays hidden until the content overflows. The
    // stock style keeps AsNeeded bars visible and only fades the thumb.
    policy: ScrollBar.AsNeeded
    visible: policy === ScrollBar.AlwaysOn || (size > 0 && size < 1 && policy !== ScrollBar.AlwaysOff)
    // The conversation list and thread reader fade their bar in while the
    // pointer moves over the view, while the user scrolls it, and while the
    // bar is hovered or dragged; it fades out after a short rest. Refreshes
    // that move the content on their own leave it hidden.
    // Its gutter stays reserved and live, so rows never shift and pointing
    // at the edge brings the bar straight back.
    property bool autoHide: false
    property bool awake: false
    hoverEnabled: true
    readonly property bool shown: !autoHide || awake || hovered || pressed
    function wake() {
        if (!autoHide) return
        awake = true
        rest.restart()
    }
    // The view's pointer, in scene coordinates. Qt repeats hover updates each
    // frame while the scene changes, so only real movement wakes the bar.
    property point pointer: Qt.point(-1, -1)
    function pointerMoved(at) {
        if (Math.abs(at.x - pointer.x) < 1 && Math.abs(at.y - pointer.y) < 1) return
        pointer = at
        wake()
    }
    Timer { id: rest; interval: 900; onTriggered: bar.awake = bar.hovered || bar.pressed }
    onHoveredChanged: if (autoHide) { if (hovered) awake = true; else rest.restart() }
    onPressedChanged: if (autoHide && !pressed) rest.restart()
    onActiveChanged: if (autoHide && active) wake()
    onPositionChanged: if (autoHide && (pressed || active)) wake()
    // Quick to appear, gentler to leave.
    opacity: autoHide ? 0 : 1
    onShownChanged: {
        if (!autoHide) return
        if (shown) { fadeOut.stop(); fadeIn.start() } else { fadeIn.stop(); fadeOut.start() }
    }
    NumberAnimation { id: fadeIn; target: bar; property: "opacity"; to: 1; duration: Theme.ms(90); easing.type: Easing.OutQuad }
    NumberAnimation { id: fadeOut; target: bar; property: "opacity"; to: 0; duration: Theme.ms(260); easing.type: Easing.InOutQuad }
    minimumSize: 0.055
    padding: 3
    implicitWidth: horizontal ? 100 : 14
    implicitHeight: horizontal ? 14 : 100
    x: !horizontal && parent ? parent.width - width : 0
    y: horizontal && parent ? parent.height - height : 0
    width: horizontal && parent ? parent.width : implicitWidth
    height: !horizontal && parent ? parent.height : implicitHeight
    contentItem: Rectangle {
        implicitWidth: 8; implicitHeight: 8; radius: 4
        color: bar.pressed ? Theme.accent : bar.hovered ? Theme.secondary : Theme.subtle
        opacity: bar.size < 1 ? 0.9 : 0.35
    }
    background: Rectangle {
        color: Theme.line; radius: 5
        // An auto-hiding bar shows its track only while pointed at.
        opacity: !bar.autoHide ? 0.6 : bar.hovered || bar.pressed ? 0.6 : 0
        Behavior on opacity { enabled: bar.autoHide; NumberAnimation { duration: Theme.ms(120); easing.type: Theme.enterEasing } }
    }
}
