import QtQuick
import QtQuick.Controls

ScrollBar {
    id: bar
    objectName: "persistentScrollBar"
    // Always-on tracks belong to the conversation list and thread reader.
    // Everywhere else the control stays hidden until the content overflows.
    // The stock style keeps AsNeeded bars visible and only fades the thumb.
    policy: ScrollBar.AsNeeded
    visible: policy === ScrollBar.AlwaysOn || (size > 0 && size < 1 && policy !== ScrollBar.AlwaysOff)
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
        color: Theme.line; opacity: 0.6; radius: 5
    }
}
