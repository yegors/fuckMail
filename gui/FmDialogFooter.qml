// Button row for an FmDialog that supplies its own buttons: right-aligned,
// with the margins, spacing and button size of FmDialog's standard buttons.
// Fill it with FmDialogFooter.Button; `primary` marks the filled main action.
import QtQuick
import QtQuick.Layouts

Item {
    id: footer
    default property alias buttons: row.data
    implicitWidth: row.implicitWidth + 44
    implicitHeight: 32 + 14 + 18

    component Button: FmButton {
        property bool primary: false
        implicitHeight: 32
        leftPadding: 14; rightPadding: 14
        font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium
        prominent: primary
        outlined: !primary
    }

    RowLayout {
        id: row
        anchors { left: parent.left; right: parent.right; top: parent.top; leftMargin: 22; rightMargin: 22; topMargin: 14 }
        spacing: 6
        Item { Layout.fillWidth: true }
    }
}
