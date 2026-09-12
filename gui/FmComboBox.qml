import QtQuick
import QtQuick.Controls

ComboBox {
    id: control
    implicitWidth: 150
    implicitHeight: 34
    leftPadding: 11
    rightPadding: 32
    font.family: Theme.fontFamily
    font.pixelSize: 13
    hoverEnabled: true
    contentItem: Text { text: control.displayText; color: control.enabled ? Theme.text : Theme.subtle; font: control.font; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight }
    indicator: FmIcon { x: control.width - width - 10; y: (control.height - height)/2; name: "down"; size: 12 }
    background: Rectangle { color: control.hovered ? Theme.hover : Theme.surface; radius: 5; border.color: control.activeFocus ? Theme.accent : Theme.controlBorder }
    delegate: ItemDelegate {
        id: option
        required property int index
        required property var modelData
        width: control.width
        implicitHeight: 34
        contentItem: Text { text: control.textRole ? modelData[control.textRole] : modelData; color: Theme.text; font: control.font; verticalAlignment: Text.AlignVCenter }
        background: Rectangle { color: option.highlighted ? Theme.selected : "transparent" }
        highlighted: control.highlightedIndex === index
    }
    popup: Popup {
        y: control.height + 4
        width: control.width
        padding: 4
        implicitHeight: Math.min(280, contentItem.implicitHeight + 8)
        contentItem: ListView { clip: true; implicitHeight: contentHeight; model: control.popup.visible ? control.delegateModel : null; currentIndex: control.highlightedIndex; ScrollIndicator.vertical: ScrollIndicator { } }
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 5 }
    }
}
