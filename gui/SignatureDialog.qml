import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// "Signature · {Account}", opened from the card's Signature row. Saves through
// the account's metadata; a refused save keeps the window and the text.
FmDialog {
    id: dialog
    objectName: "signatureDialog"
    property string account: ""
    readonly property var row: backend.allAccounts.find(a => a.id === account) || ({})
    readonly property string saved: row.signature_text || ""
    property bool saving: false
    property string error: ""
    readonly property bool dirty: editor.text !== saved
    width: Math.min(560, parent ? parent.width - 48 : 560)
    padding: 24; topPadding: 22; bottomPadding: 20
    function show(accountId) { account = accountId; error = ""; saving = false; editor.text = saved; open(); editor.forceActiveFocus() }
    function save() {
        if (!dirty || saving) return
        const name = row.name || ""
        // A signature of only whitespace is none; trailing blank lines are dropped.
        const text = editor.text.trim().length ? editor.text.replace(/\s+$/, "") : ""
        saving = true; error = ""
        backend.updateAccountPreferences(account, name, text)
    }
    Connections {
        target: backend
        function onAccountPreferencesFinished(account, ok) {
            if (!dialog.opened || !dialog.saving || account !== dialog.account) return
            dialog.saving = false
            if (ok) dialog.close()
            else dialog.error = backend.preferencesStatus.replace(/^Not saved — /, "") || "The signature was not saved."
        }
    }
    contentItem: ColumnLayout {
        spacing: 14
        RowLayout {
            Layout.fillWidth: true; spacing: 16
            ColumnLayout {
                Layout.fillWidth: true; spacing: 3
                Text { text: "Signature · " + (dialog.row.name || ""); Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(18); weight: Font.Medium; letterSpacing: -0.18 } }
                Text { text: "Added to new messages and replies from " + (dialog.row.email || ""); Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(.5); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12.5) } }
            }
            FmButton { objectName: "closeSignatureDialog"; iconName: "close"; iconSize: 12; tip: "Close — Esc"; implicitWidth: 28; implicitHeight: 28; leftPadding: 0; rightPadding: 0; Layout.alignment: Qt.AlignTop; Layout.rightMargin: -6; onClicked: dialog.close() }
        }
        ScrollView {
            Layout.fillWidth: true; Layout.preferredHeight: Math.max(150, Math.min(360, editor.contentHeight + 26))
            ScrollBar.vertical: FmScrollBar {}
            TextArea {
                id: editor
                objectName: "signature_" + dialog.account
                placeholderText: "No signature"; placeholderTextColor: Theme.faint
                padding: 12; leftPadding: 14; rightPadding: 14
                color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(14) }
                textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap; selectByMouse: true
                enabled: !dialog.saving
                background: Rectangle { color: Theme.canvas; radius: 6; border.color: editor.activeFocus ? Theme.focusBorder : Theme.line2 }
                Accessible.name: "Signature for " + (dialog.row.email || "")
            }
        }
        Text { objectName: "signatureError"; visible: dialog.error.length > 0; text: dialog.error; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.error; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
        RowLayout {
            Layout.fillWidth: true; spacing: 8
            Text { text: "Leave empty for none. Existing drafts keep theirs."; Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.inkAlpha(.45); font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) } }
            FmButton { text: "Cancel"; outlined: true; implicitHeight: 32; onClicked: dialog.close() }
            FmButton { objectName: "saveSignature_" + dialog.account; text: dialog.saving ? "Saving…" : "Save"; prominent: true; implicitHeight: 32; enabled: dialog.dirty && !dialog.saving; opacity: enabled ? 1 : .35; onClicked: dialog.save() }
        }
    }
}
