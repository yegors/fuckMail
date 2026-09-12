import QtQuick
import QtQuick.Controls

Control {
    id: control
    property var model: []
    property var optionNames: []
    property int currentIndex: 0
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
    background: Rectangle { radius: 6; color: Theme.surface; border.color: control.activeFocus ? Theme.focusBorder : "transparent" }
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
                height: Math.max(28, contentItem.implicitHeight + 10); text: String(modelData); implicitWidth: contentItem.implicitWidth + 24
                font.pixelSize: Theme.fontSize(12.5)
                selected: control.currentIndex === index
                focusPolicy: Qt.NoFocus
                onClicked: { control.forceActiveFocus(); control.selectIndex(index) }
                background: Rectangle { radius: 4; color: segment.selected ? Theme.hover : segment.hovered ? Theme.selected : "transparent" }
            }
        }
    }
}
