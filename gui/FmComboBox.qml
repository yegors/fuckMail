// The app's themed ComboBox (closed field, popup list and rows), used by
// settings pages and the event forms.
import QtQuick
import QtQuick.Controls

ComboBox {
    id: control
    // Closed-field colour; forms on raised panels use the canvas.
    property color fill: Theme.surface
    // The open list may be wider than the field, for long names.
    property real popupWidth: width
    property int textElide: Text.ElideRight
    implicitWidth: 150
    implicitHeight: 34
    leftPadding: 11
    rightPadding: 32
    font.family: Theme.fontFamily
    font.pointSize: Theme.fontPoints(13)
    hoverEnabled: true
    contentItem: Text { text: control.displayText; color: control.enabled ? Theme.text : Theme.subtle; font: control.font; verticalAlignment: Text.AlignVCenter; elide: control.textElide }
    indicator: FmIcon { x: control.width - width - 10; y: (control.height - height)/2; name: "down"; size: 12 }
    background: Rectangle { color: control.hovered ? Theme.hover : control.fill; radius: 5; border.color: control.visualFocus ? Theme.accent : Theme.controlBorder }
    delegate: ItemDelegate {
        id: option
        required property int index
        required property var modelData
        width: ListView.view ? ListView.view.width : control.width
        implicitHeight: 34
        text: control.textRole ? modelData[control.textRole] : modelData
        contentItem: Text { text: option.text; color: Theme.text; font: control.font; verticalAlignment: Text.AlignVCenter; elide: control.textElide }
        background: Rectangle { color: option.highlighted ? Theme.selected : "transparent" }
        highlighted: control.highlightedIndex === index
    }
    popup: Popup {
        y: control.height + 4
        width: Math.max(control.width, control.popupWidth)
        padding: 4
        implicitHeight: Math.min(280, contentItem.implicitHeight + 8)
        contentItem: ListView { clip: true; implicitHeight: contentHeight; model: control.popup.visible ? control.delegateModel : null; currentIndex: control.highlightedIndex; ScrollIndicator.vertical: ScrollIndicator { } }
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 5 }
    }
}
