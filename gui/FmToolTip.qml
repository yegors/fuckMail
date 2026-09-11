import QtQuick
import QtQuick.Controls
ToolTip {
    id: tooltip
    delay: 600
    padding: 8
    contentItem: Text {
        text: tooltip.text
        textFormat: Text.PlainText
        color: Theme.text
        font.pixelSize: 11
        wrapMode: Text.Wrap
    }
    background: Rectangle { color: Theme.hover; radius: 4; border.color: Theme.line }
}
