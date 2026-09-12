import QtQuick
import QtQuick.Controls
ToolButton {
    id: control
    property bool selected: false
    property bool prominent: false
    property bool outlined: false
    property string tip: ""
    property string iconName: ""
    property int iconSize: 14
    property int iconSpacing: 7
    property color foreground: !enabled ? (prominent ? Theme.muted : Theme.subtle) : prominent ? Theme.canvas : selected || outlined ? Theme.text : Theme.muted
    property int textAlignment: Text.AlignHCenter
    implicitHeight: Math.max(32, contentItem.implicitHeight + 10)
    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    leftPadding: 11
    rightPadding: 11
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontSize(13)
    font.weight: selected || prominent ? Font.Medium : Font.Normal
    hoverEnabled: true
    contentItem: Item {
        implicitWidth: label.implicitWidth + (control.iconName.length ? control.iconSize + (control.text.length ? control.iconSpacing : 0) : 0)
        implicitHeight: Math.max(label.implicitHeight, control.iconSize)
        Row {
            anchors.verticalCenter: parent.verticalCenter
            x: control.textAlignment === Text.AlignLeft ? 0 : Math.max(0, (parent.width - width) / 2)
            spacing: control.text.length && control.iconName.length ? control.iconSpacing : 0
            width: Math.min(label.implicitWidth + (control.iconName.length ? control.iconSize + (control.text.length ? control.iconSpacing : 0) : 0), parent.width)
            FmIcon { visible: control.iconName.length > 0; name: control.iconName; size: control.iconSize; color: control.foreground; anchors.verticalCenter: parent.verticalCenter }
            Text {
                id: label
                text: control.text
                font: control.font
                width: Math.max(0, parent.width - (control.iconName.length ? control.iconSize + parent.spacing : 0))
                color: control.foreground
                verticalAlignment: Text.AlignVCenter
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
        }
    }
    background: Rectangle {
        radius: 5
        color: control.prominent ? (control.enabled ? Theme.text : Theme.controlBorder) : control.down || control.selected ? Theme.selected : control.hovered ? Theme.surface : "transparent"
        border.width: control.activeFocus || control.outlined ? 1 : 0
        border.color: control.activeFocus ? Theme.focusBorder : Theme.controlBorder
    }
    FmToolTip { visible: control.hovered && control.tip.length > 0; text: control.tip }
    Accessible.name: text.length ? text : tip
}
