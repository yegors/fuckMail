import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

// "Import key": a key file or pasted armored text, holding public keys, a
// secret key or a revocation certificate. A protected secret key asks for its
// passphrase once; it is then kept sealed in the keyring, not with that
// passphrase.
FmDialog {
    id: dialog
    objectName: "pgpImportDialog"
    property url file: ""
    property string ticket: ""
    property string error: ""
    property var protectedKey: null
    readonly property bool working: ticket.length > 0
    title: "Import key"
    width: Math.min(560, parent ? parent.width - 48 : 560)
    closePolicy: working ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    function show() { file = ""; pasted.text = ""; passphrase.text = ""; protectedKey = null; error = ""; ticket = ""; open() }
    function run() {
        if (working) return
        error = ""
        ticket = backend.pgpKeys.importKeys(pasted.text, file, passphrase.text)
    }
    Connections {
        target: backend.pgpKeys
        function onFinished(ticket, action, result, ok) {
            if (ticket !== dialog.ticket) return
            dialog.ticket = ""
            if (ok) { passphrase.text = ""; dialog.close(); return }
            if (result.code === "passphrase_required" || result.code === "wrong_passphrase") {
                dialog.protectedKey = result.details || dialog.protectedKey || {}
                if (result.code === "wrong_passphrase") dialog.error = result.message
                passphrase.forceActiveFocus()
                return
            }
            dialog.error = result.message || "Nothing was imported."
        }
    }
    FileDialog {
        id: picker
        fileMode: FileDialog.OpenFile
        nameFilters: ["OpenPGP keys (*.asc *.gpg *.pgp *.key *.pub *.sec)", "All files (*)"]
        onAccepted: { dialog.file = selectedFile; pasted.text = ""; dialog.protectedKey = null; dialog.error = "" }
    }
    contentItem: ColumnLayout {
        spacing: 12
        RowLayout {
            Layout.fillWidth: true; spacing: 10
            FmButton { objectName: "pgpImportChoose"; text: "Choose file…"; outlined: true; enabled: !dialog.working; onClicked: picker.open() }
            Text {
                Layout.fillWidth: true; elide: Text.ElideMiddle; textFormat: Text.PlainText
                text: dialog.file.toString().length ? decodeURIComponent(dialog.file.toString().replace(/^file:\/\//, "")) : "or paste a key below"
                color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
        }
        ScrollView {
            Layout.fillWidth: true; Layout.preferredHeight: 150
            visible: !dialog.file.toString().length
            ScrollBar.vertical: FmScrollBar {}
            TextArea {
                id: pasted
                objectName: "pgpImportText"
                ContextMenu.menu: null
                TextMenuHandler {}
                enabled: !dialog.working
                placeholderText: "-----BEGIN PGP PUBLIC KEY BLOCK-----"; placeholderTextColor: Theme.faint
                padding: 10; color: Theme.text; font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(11)
                textFormat: TextEdit.PlainText; wrapMode: TextEdit.WrapAnywhere; selectByMouse: true
                background: Rectangle { color: Theme.canvas; radius: 5; border.color: pasted.activeFocus ? Theme.focusBorder : Theme.line2 }
                onTextChanged: { dialog.protectedKey = null; dialog.error = "" }
                Accessible.name: "Key text"
            }
        }
        ColumnLayout {
            visible: !!dialog.protectedKey
            Layout.fillWidth: true; spacing: 6
            Text {
                Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: "The secret key of " + (((dialog.protectedKey || {}).user_ids || [])[0] || "this key") + " is protected. Enter its passphrase to import it."
                color: Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
            FmTextField { id: passphrase; objectName: "pgpImportPassphrase"; Layout.fillWidth: true; echoMode: TextInput.Password; enabled: !dialog.working; placeholderText: "Passphrase"; Keys.onReturnPressed: dialog.run(); Keys.onEnterPressed: dialog.run(); Accessible.name: "Passphrase of the secret key" }
        }
        Text { objectName: "pgpImportError"; visible: dialog.error.length > 0; text: dialog.error; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.error; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
    }
    footer: RowLayout {
        spacing: 6
        Item { Layout.fillWidth: true }
        FmButton { text: "Cancel"; outlined: true; implicitHeight: 32; enabled: !dialog.working; onClicked: { passphrase.text = ""; dialog.close() } }
        FmButton { objectName: "pgpImportRun"; text: dialog.working ? "Importing…" : "Import"; prominent: true; implicitHeight: 32; enabled: backend.connected && !dialog.working && (dialog.file.toString().length > 0 || pasted.text.trim().length > 0) && (!dialog.protectedKey || passphrase.text.length > 0); onClicked: dialog.run() }
        Item { Layout.preferredWidth: 16 }
    }
    bottomPadding: 18
}
