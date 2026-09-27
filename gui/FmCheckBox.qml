// Styled checkbox row: the label, an optional grey `description` under it and
// a bottom rule. Used in form dialogs (AccountEditor, GoogleConnectionDialog).
import QtQuick
import QtQuick.Controls

CheckBox {
    id: control
    property string description: ""
    implicitHeight: Math.max(20, copy.implicitHeight) + 28
    spacing: 12; leftPadding: 0; rightPadding: 0; topPadding: 14; bottomPadding: 14
    indicator: Rectangle {
        x: control.leftPadding; y: control.topPadding + 2
        width: 16; height: 16; radius: 4
        color: control.checked ? Theme.text : "transparent"
        border.color: control.visualFocus ? Theme.focusBorder : control.checked ? Theme.text : Theme.line3
        opacity: control.enabled ? 1 : 0.4
        FmIcon { anchors.centerIn: parent; name: "check"; size: 11; color: Theme.canvas; visible: control.checked }
    }
    contentItem: Column {
        id: copy
        leftPadding: 28
        spacing: 3
        Text { width: parent.width - 28; text: control.text; color: control.enabled ? Theme.text : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(14); wrapMode: Text.Wrap; textFormat: Text.PlainText }
        Text { width: parent.width - 28; visible: text.length > 0; text: control.description; color: Theme.inkAlpha(0.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); wrapMode: Text.Wrap; lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontPixels(font) * 1.45); topPadding: Theme.leadingTop(font, lineHeight); bottomPadding: Theme.leadingBottom(font, lineHeight); textFormat: Text.PlainText }
    }
    background: Rectangle { color: "transparent"; Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.line } }
}
