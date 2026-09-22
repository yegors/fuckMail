import QtQuick
import QtQuick.Controls

Item {
    id: count
    required property string date
    required property var activity
    property bool compact: false
    property bool combined: true
    readonly property var day: (activity.days || []).find(function(row) { return row.date === count.date })
    readonly property bool ready: activity.state === "ready" && !!day
    readonly property string label: ready
        ? (compact ? "↓ " + day.received + "   ↑ " + day.sent : day.received + " received · " + day.sent + " sent")
        : activity.state === "loading" ? "Mail …" : "Mail —"
    readonly property string explanation: ready
        ? date + ": " + day.received + " emails received, " + day.sent + " sent.\n"
          + (combined ? "All active mail accounts" : "Selected mail account") + " · " + activity.time_zone
          + ".\nCached mail only; undownloaded history and drafts are not counted. Sent mail appears after sync. Google arrival dates are used when available; otherwise the message date."
        : activity.state === "display_zone_required" ? "Choose a Calendar display time zone to see daily mail counts."
        : activity.message || "Loading daily cached mail counts…"
    implicitHeight: 20
    implicitWidth: labelText.implicitWidth + 17
    activeFocusOnTab: true
    Accessible.role: Accessible.StaticText
    Accessible.name: explanation
    FmIcon { name: "mail"; size: 12; anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter }
    Text {
        id: labelText
        anchors.fill: parent
        anchors.leftMargin: 17
        text: count.label
        textFormat: Text.PlainText
        color: Theme.muted
        font.family: Theme.fontFamily
        font.pixelSize: Theme.fontSize(11)
        elide: Text.ElideRight
        verticalAlignment: Text.AlignVCenter
    }
    HoverHandler { id: hover }
    FmToolTip { visible: hover.hovered || count.activeFocus; text: count.explanation }
}
