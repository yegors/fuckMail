// The app's themed single-line TextField; FmDateField and most form inputs
// build on it.
import QtQuick
import QtQuick.Controls

TextField {
    id: field
    implicitHeight: Math.max(34, contentHeight + topPadding + bottomPadding)
    padding: 9
    color: Theme.text
    placeholderTextColor: Theme.subtle
    font.family: Theme.fontFamily
    font.pointSize: Theme.fontPoints(13)
    selectionColor: Theme.selection
    selectedTextColor: Theme.selectedText
    selectByMouse: true
    background: Rectangle { color: Theme.surface; radius: 5; border.color: field.activeFocus ? Theme.accent : Theme.controlBorder }
}
