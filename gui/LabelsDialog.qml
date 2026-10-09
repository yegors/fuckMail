import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// "Labels · {Account}", opened from the account's Labels row in Accounts.
// The table is the LabelSettings editor: name, description, Read, Spam,
// remove, add, and Sync with Google. Esc or a click outside closes it.
FmDialog {
    id: dialog
    objectName: "labelsDialog"
    property string account: ""
    readonly property var row: backend.allAccounts.find(a => a.id === account) || ({})
    signal changed()
    width: Math.min(720, parent ? parent.width - 48 : 720)
    height: Math.min(implicitHeight, parent ? parent.height - 96 : implicitHeight)
    padding: 28; topPadding: 22; bottomPadding: 26
    function show(accountId) { account = accountId; open() }
    onClosed: changed()
    contentItem: ScrollView {
        ScrollBar.vertical: FmScrollBar {}
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        contentWidth: availableWidth; clip: true
        implicitHeight: body.implicitHeight
        ColumnLayout {
            id: body
            width: parent.width; spacing: 0
            RowLayout {
                Layout.fillWidth: true; spacing: 16
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 3
                    Text { objectName: "labelsDialogTitle"; text: "Labels · " + (dialog.row.name || ""); Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(17); weight: Font.Medium; letterSpacing: 0} }
                    Text { text: dialog.row.email || ""; Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
                }
                FmButton { objectName: "closeLabelsDialog"; iconName: "close"; iconSize: 12; tip: "Close — Esc"; implicitWidth: 28; implicitHeight: 28; leftPadding: 0; rightPadding: 0; Layout.alignment: Qt.AlignTop; Layout.rightMargin: -6; onClicked: dialog.close() }
            }
            Text {
                Layout.fillWidth: true; Layout.maximumWidth: 600; Layout.topMargin: 16
                text: "Labels for " + (dialog.row.name || "this account") + " only. Descriptions help the local model sort mail."
                wrapMode: Text.Wrap; color: Theme.muted; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) }
                lineHeightMode: Text.FixedHeight; lineHeight: Math.round(Theme.fontSize(12.5) * 1.5)
            }
            LabelSettings {
                Layout.fillWidth: true; Layout.maximumWidth: 640; Layout.topMargin: 14
                visible: dialog.opened; account: dialog.account; showAccountPicker: false; showHeading: false
            }
        }
    }
}
