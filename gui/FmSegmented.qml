// Segmented control: a row of mutually exclusive buttons built from `model`,
// with arrow / Home / End keyboard selection. Emits activated(index).
import QtQuick
import QtQuick.Controls

Control {
    id: control
    property var model: []
    property var optionNames: []
    property int currentIndex: 0
    property bool compact: false
    property bool hoverHighlight: !compact
    property color fill: compact ? Theme.canvas : Theme.surface
    signal activated(int index)
    padding: 2
    implicitWidth: segments.implicitWidth + 4
    implicitHeight: segments.implicitHeight + 4
    focusPolicy: Qt.StrongFocus
    function selectIndex(index) {
        const next = Math.max(0, Math.min(model.length - 1, index))
        currentIndex = next
        activated(next)
    }
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Left || event.key === Qt.Key_Up) selectIndex(currentIndex - 1)
        else if (event.key === Qt.Key_Right || event.key === Qt.Key_Down) selectIndex(currentIndex + 1)
        else if (event.key === Qt.Key_Home) selectIndex(0)
        else if (event.key === Qt.Key_End) selectIndex(model.length - 1)
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) activated(currentIndex)
        else return
        event.accepted = true
    }
    background: Rectangle { radius: 6; color: control.fill; border.color: control.activeFocus ? Theme.line3 : "transparent" }
    contentItem: Row {
        id: segments
        spacing: 2
        Repeater {
            model: control.model
            FmButton {
                required property var modelData
                required property int index
                id: segment
                objectName: control.optionNames[index] || ""
                height: Math.max(control.compact ? 22 : 28, contentItem.implicitHeight + (control.compact ? 6 : 10)); text: String(modelData); implicitWidth: contentItem.implicitWidth + (control.compact ? 20 : 24)
                leftPadding:control.compact ? 10 : 12;rightPadding:leftPadding
                font.pointSize: Theme.fontPoints(control.compact ? 12 : 12.5)
                selected: control.currentIndex === index
                focusPolicy: Qt.NoFocus
                onClicked: { control.forceActiveFocus(); control.selectIndex(index) }
                background: Rectangle { radius: 4; color: segment.selected ? Theme.brighten(Theme.hover, segment.lift) : control.hoverHighlight && segment.hovered ? Theme.selected : "transparent" }
            }
        }
    }
}
