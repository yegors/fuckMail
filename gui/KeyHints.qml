// The shortcut bar along the bottom of the window: a 24px row of key chips
// for the current view (Mail, Calendar or Settings) at the left and the keys
// that work everywhere at the right. Keys come from ShortcutRegistry, so a
// remapped key shows as remapped.
import QtQuick
import QtQuick.Layouts

Rectangle {
    id: bar
    // "mail", "calendar" or "settings".
    property string view: "mail"
    function key(id) { return ShortcutRegistry.label(ShortcutRegistry.sequence(id, backend.preferences)).toLowerCase() }
    readonly property var hints: view === "settings" ? [["esc", "close"]]
        : view === "calendar" ? [[key("day"), "day"], [key("week"), "week"], [key("month"), "month"], [key("back") + " " + key("fwd"), "move"], [key("today"), "today"], [key("newEv"), "new event"]]
        : [[key("next") + " " + key("prev"), "move"], [key("done"), "done"], [key("delay"), "delay"], [key("reply"), "reply"], [key("replyAll"), "reply all"],
           [key("forward"), "forward"], [key("unread"), "unread"], [key("scopes"), "folders"], [key("search"), "search"]]
    readonly property var everywhere: [[key("view"), view === "calendar" ? "mail" : "calendar"], [key("settings"), "settings"], ["~", "console"]]
    implicitHeight: visible ? Theme.keyHintHeight : 0
    height: implicitHeight
    color: Theme.canvas
    clip: true
    Rectangle { x: Theme.snapX(parent, 0); y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line }
    component Hint: Row {
        property var hint: ["", ""]
        spacing: 6
        Rectangle {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(16, keyText.implicitWidth + 8); height: 16; radius: 4; color: Theme.inkAlpha(.1)
            Text { id: keyText; anchors.centerIn: parent; text: hint[0]; color: Theme.text; font { family: backend.fixedFontFamily(); pointSize: Theme.fontPoints(11) } }
        }
        Text { anchors.verticalCenter: parent.verticalCenter; text: hint[1]; color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) } }
    }
    RowLayout {
        anchors.fill: parent; anchors.leftMargin: 16; anchors.rightMargin: 16; spacing: 18
        Repeater { model: bar.hints; Hint { required property var modelData; hint: modelData } }
        Item { Layout.fillWidth: true }
        Repeater { model: bar.everywhere; Hint { required property var modelData; hint: modelData } }
    }
}
