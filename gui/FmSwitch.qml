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
            // The mark pops in a touch past full size and settles.
            Rectangle {
                visible: control.checked || scale > .01; x: 3; y: 3; width: 6; height: 6; color: control.tone
                scale: control.checked ? 1 : 0
                Behavior on scale { NumberAnimation { duration: Theme.ms(Theme.quickMs); easing.type: control.checked ? Easing.OutBack : Theme.exitEasing; easing.overshoot: 2.5 } }
            }
        }
        Rectangle {
            visible: control.pill
            anchors.fill: parent; radius: 5; color: control.tone
            border.width: control.visualFocus ? 1 : 0; border.color: Theme.focusBorder
            // The knob stretches along its path while it travels and is round
            // again when it lands, so it reads as moving rather than jumping.
            Rectangle {
                objectName: "switchKnob"
                readonly property real size: parent.height - 4
                property real travel: control.checked ? 1 : 0
                Behavior on travel { NumberAnimation { duration: Theme.ms(Theme.baseMs); easing.type: Theme.moveEasing } }
                readonly property real stretch: 5 * Math.sin(travel * Math.PI)
                width: size + stretch; height: size; radius: 5; y: 2
                x: 2 + travel * (parent.width - size - 4) - stretch / 2
                color: control.checked ? Theme.canvas : Theme.inkAlpha(0.7)
            }
        }
    }
    background: null
}
