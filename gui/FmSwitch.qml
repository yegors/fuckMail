// Small on/off toggle, exposed to accessibility as a checkbox. By default a
// 12px bordered square that fills with a 6px square when on; `pill` keeps
// the 30×18 sliding switch for form rows (All day, settings).
import QtQuick
import QtQuick.Controls

AbstractButton {
    id: control
    property bool pill: false
    checkable: true
    implicitWidth: pill ? 30 : 12; implicitHeight: pill ? 18 : 12
    hoverEnabled: true
    Accessible.role: Accessible.CheckBox
    Accessible.checkable: true
    Accessible.checked: checked
    readonly property color tone: checked ? Theme.text : Theme.line3
    contentItem: Item {
        opacity: control.enabled ? 1 : 0.4
        Rectangle {
            visible: !control.pill
            anchors.fill: parent; color: "transparent"
            border.width: 1; border.color: control.visualFocus ? Theme.focusBorder : control.tone
            Rectangle { visible: control.checked; x: 3; y: 3; width: 6; height: 6; color: control.tone }
        }
        Rectangle {
            visible: control.pill
            anchors.fill: parent; radius: 5; color: control.tone
            border.width: control.visualFocus ? 1 : 0; border.color: Theme.focusBorder
            Rectangle {
                width: parent.height - 4; height: width; radius: 5; y: 2
                x: control.checked ? parent.width - width - 2 : 2
                color: control.checked ? Theme.canvas : Theme.inkAlpha(0.7)
                Behavior on x { NumberAnimation { duration: 150 } }
            }
        }
    }
    background: null
}
