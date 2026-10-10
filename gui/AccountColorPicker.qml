import QtQuick
import QtQuick.Controls

// The account colour picker, opened under the card's avatar: 16 muted swatches
// in an 8×2 grid with the current one ringed. The colour marks the account's
// top-bar badge, card avatar and calendar events; GUI and CLI share it.
Popup {
    id: picker
    // Eases in from just under full size; closes at once.
    transformOrigin: Popup.TopLeft
    enter: Transition { ParallelAnimation { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.ms(Theme.quickMs); easing.type: Theme.enterEasing } } }
    objectName: "accountColorPicker"
    property string account: ""
    parent: Overlay.overlay
    padding: 8
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    function show(accountId, left, top) { account = accountId; x = left; y = top; open() }
    function pick(hex) {
        const row = backend.allAccounts.find(a => a.id === account)
        if (row && hex !== Theme.accountColor(account).toUpperCase())
            backend.updateAccountPreferences(account, row.name, row.signature_text || "", "", {}, false, hex)
        close()
    }
    background: Rectangle { radius: 8; color: Theme.bg2; border.color: Theme.line2 }
    contentItem: Grid {
        columns: 8; spacing: 8
        Repeater {
            model: Theme.accountPalette
            AbstractButton {
                id: swatch
                required property string modelData
                objectName: "accountColor_" + modelData.slice(1)
                readonly property bool current: Theme.accountColor(picker.account).toUpperCase() === modelData
                width: 22; height: 22
                scale: hovered ? 1.12 : 1
                Behavior on scale { NumberAnimation { duration: Theme.ms(100); easing.type: Theme.enterEasing } }
                onClicked: picker.pick(modelData)
                // The ring sits outside the swatch with a 2px gap of the menu colour.
                background: Item {
                    Rectangle { anchors.centerIn: parent; visible: swatch.current; width: 29; height: 29; radius: 14.5; color: swatch.modelData }
                    Rectangle { anchors.centerIn: parent; visible: swatch.current; width: 26; height: 26; radius: 13; color: Theme.bg2 }
                    Rectangle { anchors.fill: parent; radius: 11; color: swatch.modelData }
                }
                Accessible.name: "Account colour " + modelData
                Accessible.role: Accessible.RadioButton
                Accessible.checked: current
                HoverHandler { cursorShape: Qt.PointingHandCursor }
                FmToolTip { shown: swatch.hovered; text: swatch.modelData }
            }
        }
    }
}
