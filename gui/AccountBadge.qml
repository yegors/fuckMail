import QtQuick
import QtQuick.Controls
import QtQuick.Shapes

// An account's letter in its colour: stronger when selected. The top-bar tab
// adds a progress ring (amber downloading, silver-blue model processing) and a
// red dot when the account is disconnected.
Item {
    id: badge
    property string account: ""
    property string accountName: ""
    property bool selected: false
    property int diameter: 18
    property bool ringShown: false
    property real ringProgress: 0
    property color ringColor: Theme.ai
    property bool errorShown: false
    property bool tipEnabled: true
    // Keep room for the ring so a tab does not shift when a stage starts.
    property bool reserveRing: false
    readonly property color hue: Qt.color(Theme.accountColor(account))
    implicitWidth: reserveRing || ringShown || errorShown ? diameter + 4 : diameter
    implicitHeight: implicitWidth
    width: implicitWidth; height: implicitHeight
    Shape {
        visible: badge.ringShown
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "transparent"; strokeColor: Theme.inkAlpha(.1); strokeWidth: 1.5
            PathAngleArc { centerX: badge.width / 2; centerY: badge.height / 2; radiusX: badge.width / 2 - 1; radiusY: radiusX; startAngle: 0; sweepAngle: 360 }
        }
        ShapePath {
            fillColor: "transparent"; strokeColor: badge.ringColor; strokeWidth: 1.5; capStyle: ShapePath.RoundCap
            PathAngleArc { centerX: badge.width / 2; centerY: badge.height / 2; radiusX: badge.width / 2 - 1; radiusY: radiusX; startAngle: -90; sweepAngle: 360 * Math.max(0.02, Math.min(1, badge.ringProgress)) }
        }
    }
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
    Rectangle {
        objectName: "accountBadgeError"
        visible: badge.errorShown
        // A 6px dot in a 2px ring of the bar colour.
        anchors.right: parent.right; anchors.top: parent.top; anchors.rightMargin: -2; anchors.topMargin: -2
        width: 10; height: 10; radius: 5; color: Theme.error
        border.width: 2; border.color: Theme.canvas
    }
    HoverHandler { id: hover; enabled: badge.tipEnabled }
    FmToolTip { shown: badge.tipEnabled && hover.hovered; text: badge.accountName }
}
