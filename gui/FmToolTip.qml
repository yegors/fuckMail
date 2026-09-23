import QtQuick
import QtQuick.Controls
ToolTip {
    id: tooltip
    delay: 600
    padding: 10
    implicitWidth: Math.min(400, contentItem.implicitWidth + leftPadding + rightPadding)
    contentItem: Text {
        text: tooltip.text
        textFormat: Text.PlainText
        color: Theme.text
        font.family: Theme.fontFamily
        font.pointSize: Theme.fontPoints(11)
        wrapMode: Text.Wrap
    }
    background: Rectangle { color: Theme.surface; radius: 5; border.color: Theme.controlBorder }
}
