import QtQuick
import QtQuick.Controls
ToolButton {
    id: control
    property bool selected: false
    property bool prominent: false
    property bool outlined: false
    property string tip: ""
    property int textAlignment: Text.AlignHCenter
    implicitHeight: 30
    implicitWidth: contentItem.implicitWidth + 22
    leftPadding: 11
    rightPadding: 11
    font.pixelSize: 12
    hoverEnabled: true
    contentItem: Text {
        text: control.text
        font: control.font
        color: !control.enabled ? Theme.subtle : control.prominent ? Theme.canvas : control.selected ? Theme.text : Theme.muted
        horizontalAlignment: control.textAlignment
        verticalAlignment: Text.AlignVCenter
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }
    background: Rectangle {
        radius: 4
        color: control.prominent ? "#e3e5e9" : control.down || control.selected ? Theme.selected : control.hovered ? Theme.hover : "transparent"
        border.width: control.activeFocus || control.outlined ? 1 : 0
        border.color: control.activeFocus ? Theme.accent : Theme.line
    }
    FmToolTip { visible: control.hovered && control.tip.length > 0; text: control.tip }
    Accessible.name: text
}
