import QtQuick
import QtQuick.Controls

ScrollBar {
    id: bar
    objectName: "persistentScrollBar"
    policy: ScrollBar.AlwaysOn
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
