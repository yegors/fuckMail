import QtQuick
import QtQuick.Controls

AbstractButton {
    id: control
    checkable: true
    implicitWidth: 30; implicitHeight: 18
    hoverEnabled: true
    Accessible.role: Accessible.CheckBox
    Accessible.checkable: true
    Accessible.checked: checked
    contentItem: Rectangle {
        radius: height / 2
        color: control.checked ? Theme.text : Theme.line3
        opacity: control.enabled ? 1 : 0.4
        border.width: control.activeFocus ? 1 : 0
        border.color: Theme.focusBorder
        Rectangle {
            width: 14; height: 14; radius: 7; y: 2
            x: control.checked ? parent.width - width - 2 : 2
            color: control.checked ? Theme.canvas : Theme.inkAlpha(0.7)
            Behavior on x { NumberAnimation { duration: 150 } }
        }
    }
    background: null
}
