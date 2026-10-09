import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// One 28px menu row: 13px label, optional dimmed key hint, optional rule above.
FmButton {
    id: item
    property string shortcut: ""
    property bool separatorAbove: false
    property bool danger: false
    // Check-mark rows; distinct from the button's own toggle state.
    property bool marked: false
    property bool markable: false
    property bool partial: false
    readonly property int rowHeight: 28
    Layout.fillWidth: true
    implicitHeight: rowHeight + (separatorAbove ? 5 : 0)
    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    leftPadding: 9; rightPadding: 9; topPadding: separatorAbove ? 5 : 0; bottomPadding: 0
    font.pointSize: Theme.fontPoints(12); font.weight: Font.Normal
    foreground: !enabled ? Theme.faint : danger ? Theme.error : Theme.text
    contentItem: RowLayout {
        spacing: 10
        FmIcon { visible: item.markable; name: "check"; size: 11; color: Theme.accent; opacity: item.marked ? (item.partial ? .4 : 1) : 0 }
        FmIcon { visible: item.iconName.length > 0 && !item.markable; name: item.iconName; size: 13; color: item.foreground }
        Text { text: item.text; color: item.foreground; font: item.font; elide: Text.ElideRight; textFormat: Text.PlainText; Layout.fillWidth: true; Layout.minimumWidth: 0 }
        Text { visible: item.shortcut.length > 0; text: item.shortcut; color: Theme.inkAlpha(.38); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); Layout.leftMargin: 8 }
    }
    background: Item {
        Rectangle { visible: item.separatorAbove; y: 4; width: parent.width; height: 1; color: Theme.line }
        Rectangle {
            y: item.topPadding; width: parent.width; height: item.rowHeight; radius: 5
            color: item.down || item.hovered || item.visualFocus ? Theme.bg4 : "transparent"
            border.width: item.activeFocus && item.visualFocus ? 1 : 0; border.color: Theme.focusBorder
        }
    }
}
