// Hover help for any item: add it as a child, set `text` and bind `shown`
// (usually to a HoverHandler). FmButton's `tip` property uses it.
import QtQuick
import QtQuick.Controls
// Hover help is created only while it is shown: most controls are never
// hovered, and a popup per button made every list and grid rebuild costly.
Item {
    id: holder
    property string text: ""
    // The item the help belongs to: the holder's parent unless loaded elsewhere.
    property Item target: parent
    // Show state; the holder itself stays invisible so layouts never reserve it.
    property bool shown: false
    visible: false
    width: 0; height: 0
    Loader {
        active: holder.shown && holder.text.length > 0
        sourceComponent: ToolTip {
            id: tooltip
            parent: holder.target
            visible: true
            delay: 600
            padding: 10
            text: holder.text
            implicitWidth: Math.min(400, contentItem.implicitWidth + leftPadding + rightPadding)
            contentItem: Text {
                text: tooltip.text
                textFormat: Text.PlainText
                color: Theme.text
                font.family: Theme.fontFamily
                font.pointSize: Theme.fontPoints(10.5)
                wrapMode: Text.Wrap
            }
            background: Rectangle { color: Theme.surface; radius: 5; border.color: Theme.controlBorder }
        }
    }
}
