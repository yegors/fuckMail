import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// "Back up secret key": the passphrase that protects the exported copy. The
// copy works in any OpenPGP app, so it needs a strong passphrase: at least 12
// characters, typed twice. Once the dialog has closed, `chosen` asks the
// caller to pick where to save it.
FmDialog {
    id: dialog
    objectName: "pgpBackupDialog"
    signal chosen(var key, string passphrase)
    property var key: ({})
    // The chosen passphrase, held only until the dialog has closed.
    property string pending: ""
    readonly property bool longEnough: first.text.length >= 12
    readonly property bool same: first.text === second.text
    title: "Back up secret key"
    width: Math.min(500, parent ? parent.width - 48 : 500)
    function show(row) { key = row || {}; pending = ""; first.text = ""; second.text = ""; open(); first.forceActiveFocus() }
    function choose() {
        if (!longEnough || !same) return
        pending = first.text
        first.text = ""; second.text = ""
        close()
    }
    // The system save dialog opens only after this one is gone.
    onClosed: {
        const passphrase = pending
        pending = ""
        if (passphrase.length) chosen(key, passphrase)
    }
    contentItem: ColumnLayout {
        spacing: 12
        Text {
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: "A copy of " + ((dialog.key.user_ids || [])[0] || "your key") + " to keep somewhere safe or use in another app. Anyone with the file and its passphrase can read your encrypted mail and sign as you."
            color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        }
        FmTextField { id: first; objectName: "pgpBackupPassphrase"; Layout.fillWidth: true; echoMode: TextInput.Password; placeholderText: "Passphrase, at least 12 characters"; Accessible.name: "Passphrase for the backup" }
        FmTextField { id: second; objectName: "pgpBackupRepeat"; Layout.fillWidth: true; echoMode: TextInput.Password; placeholderText: "The same passphrase again"; Accessible.name: "Repeat the passphrase"; Keys.onReturnPressed: dialog.choose(); Keys.onEnterPressed: dialog.choose() }
        Text {
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: first.text.length && !dialog.longEnough ? "Use at least 12 characters. A few unrelated words are easy to remember and hard to guess."
                : second.text.length && !dialog.same ? "The passphrases don't match."
                : "There's no way to recover a forgotten passphrase."
            color: (first.text.length && !dialog.longEnough) || (second.text.length && !dialog.same) ? Theme.warn : Theme.inkAlpha(.5)
            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
        }
    }
    footer: FmDialogFooter {
        FmDialogFooter.Button { objectName: "pgpBackupCancel"; text: "Cancel"; onClicked: { first.text = ""; second.text = ""; dialog.close() } }
        FmDialogFooter.Button { objectName: "pgpBackupSave"; primary: true; text: "Save backup…"; enabled: dialog.longEnough && dialog.same; onClicked: dialog.choose() }
    }
}
