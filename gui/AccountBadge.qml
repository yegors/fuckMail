import QtQuick
import QtQuick.Controls

// An account's letter in its colour, stronger when selected.
Item {
    id: badge
    property string account: ""
    property string accountName: ""
    property bool selected: false
    property int diameter: 18
    property bool tipEnabled: true
    readonly property color hue: Qt.color(Theme.accountColor(account))
    implicitWidth: diameter
    implicitHeight: implicitWidth
    width: implicitWidth; height: implicitHeight
    Rectangle {
        objectName: "accountBadgeLetter"
        anchors.centerIn: parent
        width: badge.diameter; height: badge.diameter; radius: badge.diameter / 2
        color: Theme.withAlpha(badge.hue, badge.selected ? .22 : .12)
        Text {
            anchors.centerIn: parent
            text: badge.accountName.trim().charAt(0).toUpperCase()
            color: badge.selected ? badge.hue : Theme.withAlpha(badge.hue, .75)
            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(badge.diameter <= 14 ? 8 : 10); font.weight: Font.DemiBold
        }
    }
    HoverHandler { id: hover; enabled: badge.tipEnabled }
    FmToolTip { shown: badge.tipEnabled && hover.hovered; text: badge.accountName }
}
