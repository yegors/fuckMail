import QtQuick
import QtQuick.Controls

Rectangle {
    id: count
    required property string date
    property var activity: ({})
    property string accountName: ""
    property bool compact: false
    property real textSize: 11
    signal clicked()
    // Callers holding a date map pass the day's row directly.
    property var row: activity.by_date ? activity.by_date[count.date] : (activity.days || []).find(function(row) { return row.date === count.date })
    readonly property var day: row
    readonly property bool ready: !!day && (activity.state === undefined || activity.state === "ready")
    visible: date <= backend.todayDate && ready
    implicitHeight: 20
    implicitWidth: counts.implicitWidth + 16
    radius: 10
    color: Theme.surface
    border.color: hover.hovered ? Theme.line3 : Theme.line
    activeFocusOnTab: visible
    Accessible.role: Accessible.Button
    Accessible.name: ready ? day.received + " received, " + day.sent + " sent, " + accountName + ". Open Mail" : ""
    Accessible.onPressAction: count.clicked()
    Keys.onReturnPressed: count.clicked()
    Keys.onSpacePressed: count.clicked()
    Row {
        id: counts
        anchors.centerIn: parent
        spacing: 8
        Row {
            spacing: 3
            FmIcon { name: "arrowDown"; size: 9; color: received.color; anchors.verticalCenter: parent.verticalCenter }
            Text { id: received; text: count.ready ? count.day.received : 0; color: hover.hovered ? Theme.text : Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(count.textSize); font.features: { "tnum": 1 } }
        }
        Row {
            spacing: 3
            FmIcon { name: "arrowUp"; size: 9; color: received.color; anchors.verticalCenter: parent.verticalCenter }
            Text { text: count.ready ? count.day.sent : 0; color: received.color; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(count.textSize); font.features: { "tnum": 1 } }
        }
    }
    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: count.clicked() }
    HoverHandler { id: hover }
    FmToolTip { shown: hover.hovered || count.activeFocus; text: count.ready ? count.day.received + " received · " + count.day.sent + " sent · " + count.accountName : "" }
}
