import QtQuick
import QtQuick.Controls

Rectangle {
    id: badge
    property string accountName: ""
    property bool selected: false
    property int diameter: 18
    property color idleColor: Theme.bg4
    property color idleText: Theme.inkAlpha(.6)
    implicitWidth: diameter; implicitHeight: diameter
    width: diameter; height: diameter; radius: diameter / 2
    color: selected ? Theme.accentAlpha(.2) : idleColor
    Text {
        anchors.centerIn: parent
        text: badge.accountName.trim().charAt(0).toUpperCase()
        color: badge.selected ? Theme.accent : badge.idleText
        font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(badge.diameter === 14 ? 8 : 10); font.weight: Font.DemiBold
    }
    HoverHandler { id: hover }
    FmToolTip { visible: hover.hovered; text: badge.accountName }
}
