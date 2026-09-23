import QtQuick

FmButton {
    id: button
    implicitWidth: text.length ? contentItem.implicitWidth + 16 : 26
    implicitHeight: 26; iconSize: 12
    leftPadding: text.length ? 8 : 6; rightPadding: leftPadding
    topPadding: 0; bottomPadding: 0
    font.pointSize: Theme.fontPoints(11); font.weight: Font.Medium; font.letterSpacing: .44
    foreground: hovered ? Theme.text : Theme.inkAlpha(.45)
    background: Rectangle { radius: 5; color: button.hovered ? Theme.bg4 : "transparent"; border.width: button.activeFocus ? 1 : 0; border.color: Theme.focusBorder }
}
