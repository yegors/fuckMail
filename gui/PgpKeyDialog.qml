import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// "Create key": a new OpenPGP key for one account's name and address, which
// becomes that account's key. Ed25519 with Curve25519 suits every current
// OpenPGP app; RSA 4096 is for older ones. Expires after three years unless
// changed, as GnuPG does; it can be renewed later.
FmDialog {
    id: dialog
    objectName: "pgpKeyDialog"
    signal created(string fingerprint)
    property var account: null
    property int algorithm: 0
    property int lifetime: 2
    property string ticket: ""
    property string error: ""
    readonly property var lifetimes: [365, 730, 1095, 1825, 0]
    readonly property var accounts: backend.allAccounts.filter(a => (a.lifecycle || {}).state !== "removed")
    readonly property bool working: ticket.length > 0
    title: "Create key"
    width: Math.min(520, parent ? parent.width - 48 : 520)
    closePolicy: working ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    function show(row) {
        account = row || accounts[0] || null
        name.text = account ? account.name : ""
        algorithm = 0; lifetime = 2; error = ""; ticket = ""
        open()
    }
    function create() {
        if (!account || working) return
        error = ""
        ticket = backend.pgpKeys.generate(account.id, name.text, account.email, ["curve25519", "rsa4096"][algorithm], lifetimes[lifetime])
    }
    Connections {
        target: backend.pgpKeys
        function onFinished(ticket, action, result, ok) {
            if (ticket !== dialog.ticket) return
            dialog.ticket = ""
            if (!ok) { dialog.error = result.message || "The key wasn't created."; return }
            dialog.close()
            dialog.created(result.fingerprint)
        }
    }
    contentItem: ColumnLayout {
        spacing: 14
        GridLayout {
            Layout.fillWidth: true; columns: 2; columnSpacing: 16; rowSpacing: 12
            Text { text: "Account"; color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmComboBox {
                objectName: "pgpKeyAccount"
                Layout.fillWidth: true; implicitHeight: 32
                enabled: !dialog.working
                model: dialog.accounts.map(a => a.name + " · " + a.email)
                currentIndex: Math.max(0, dialog.accounts.findIndex(a => dialog.account && a.id === dialog.account.id))
                onActivated: function(index) { dialog.account = dialog.accounts[index]; name.text = dialog.account.name }
                Accessible.name: "Account"
            }
            Text { text: "Name"; color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmTextField { id: name; objectName: "pgpKeyName"; Layout.fillWidth: true; enabled: !dialog.working; maximumLength: 80; placeholderText: "Shown to people who get your key"; Accessible.name: "Name in the key" }
            Text { text: "Type"; color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmSegmented { objectName: "pgpKeyAlgorithm"; enabled: !dialog.working; model: ["Ed25519 (recommended)", "RSA 4096"]; currentIndex: dialog.algorithm; onActivated: function(index) { dialog.algorithm = index } }
            Text { text: "Expires"; color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmSegmented { objectName: "pgpKeyLifetime"; enabled: !dialog.working; model: ["1 year", "2 years", "3 years", "5 years", "Never"]; currentIndex: dialog.lifetime; onActivated: function(index) { dialog.lifetime = index } }
        }
        Text {
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: dialog.algorithm === 1 ? "RSA keys take a few seconds to make and are larger. Choose them only for software that can't read Ed25519 keys." : "An expiry date limits how long a lost key stays usable. You can renew it any time."
            color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
        }
        Text { objectName: "pgpKeyError"; visible: dialog.error.length > 0; text: dialog.error; Layout.fillWidth: true; wrapMode: Text.Wrap; color: Theme.error; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
    }
    footer: RowLayout {
        spacing: 6
        Item { Layout.fillWidth: true }
        FmButton { text: "Cancel"; outlined: true; implicitHeight: 32; enabled: !dialog.working; onClicked: dialog.close() }
        FmButton { objectName: "pgpKeyCreate"; text: dialog.working ? "Creating…" : "Create key"; prominent: true; implicitHeight: 32; enabled: !!dialog.account && !dialog.working && backend.connected; onClicked: dialog.create() }
        Item { Layout.preferredWidth: 16 }
    }
    bottomPadding: 18
}
