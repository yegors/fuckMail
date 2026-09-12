import QtQuick
import QtQuick.Controls

TextField {
    id: field
    implicitHeight: 34
    padding: 9
    color: Theme.text
    placeholderTextColor: Theme.subtle
    font.family: Theme.fontFamily
    font.pixelSize: 13
    selectByMouse: true
    background: Rectangle { color: Theme.surface; radius: 5; border.color: field.activeFocus ? Theme.accent : Theme.controlBorder }
}
