import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts

// Settings → Encryption: the OpenPGP keyring. Your keys (create, import, back
// up, renew, delete), which key each account signs and decrypts with, and
// other people's keys for encrypting to them. PgpKeys does the engine calls.
ColumnLayout {
    id: page
    objectName: "encryptionSettings"
    readonly property var keyring: backend.pgpKeys
    readonly property string mono: backend.fixedFontFamily()
    readonly property var ownKeys: (keyring ? keyring.keys : []).filter(k => k.secret)
    readonly property var otherKeys: (keyring ? keyring.keys : []).filter(k => !k.secret)
    readonly property var accounts: backend.allAccounts.filter(a => (a.lifecycle || {}).state !== "removed")
    // The rows, by account id and fingerprint. Each save reloads the keyring
    // and the account rows; a row whose details change updates in place, and
    // the lists are rebuilt only when rows come or go.
    property var accountIds: []
    property var ownFingerprints: []
    property var otherFingerprints: []
    function same(a, b) { return a.length === b.length && a.every((v, i) => v === b[i]) }
    function syncRows() {
        const ids = page.accounts.map(a => a.id), own = page.ownKeys.map(k => k.fingerprint), other = page.otherKeys.map(k => k.fingerprint)
        if (!same(ids, accountIds)) accountIds = ids
        if (!same(own, ownFingerprints)) ownFingerprints = own
        if (!same(other, otherFingerprints)) otherFingerprints = other
    }
    onAccountsChanged: syncRows()
    onOwnKeysChanged: syncRows()
    onOtherKeysChanged: syncRows()
    Component.onCompleted: syncRows()
    property string status: ""
    property bool statusError: false
    // A keyring change failed; account rows drop the choice they showed early.
    signal saveFailed()
    // A pending export or save waits here while the file dialog is open.
    property var pendingSave: null
    readonly property bool popupOpen: createDialog.opened || importDialog.opened || backupDialog.opened || expiryDialog.opened || deleteDialog.opened || saveDialog.visible
    spacing: 0

    function say(text, error) { status = text; statusError = !!error }
    function when(seconds) { return seconds ? Qt.formatDate(new Date(seconds * 1000), "d MMM yyyy") : "" }
    function expiry(key) {
        if (key.status === "revoked") return "Revoked"
        if (!key.expires_at) return "Never expires"
        const days = Math.ceil((key.expires_at * 1000 - Date.now()) / 86400000)
        if (days <= 0) return "Expired " + when(key.expires_at)
        return (days <= 60 ? "Expires in " + days + (days === 1 ? " day" : " days") : "Expires " + when(key.expires_at))
    }
    function statusTone(key) { return key.status === "valid" ? Theme.inkAlpha(.5) : Theme.warn }
    function accountName(id) { const row = page.accounts.find(a => a.id === id); return row ? row.name : id }
    function usableFor(account) {
        const email = String(account.email || "").toLowerCase()
        return page.ownKeys.filter(k => (k.addresses || []).indexOf(email) >= 0 && k.status === "valid")
    }
    // Saves an export to a file the person picks; `kind` is public, secret or revocation.
    function saveKey(key, kind, passphrase) {
        pendingSave = {fingerprint: key.fingerprint, kind: kind, passphrase: passphrase || ""}
        saveDialog.currentFolder = page.keyring.exportFolder()
        saveDialog.selectedFile = page.keyring.exportFile(key, kind)
        saveDialog.open()
    }
    Connections {
        target: page.keyring
        ignoreUnknownSignals: true
        function onFinished(ticket, action, result, ok) {
            if (!ok) { page.say(result.message || "That didn't work.", true); page.saveFailed(); return }
            // Switching a default shows in its switch; a status line would push the page down.
            if (action === "account" && result.pgp) return
            if (action === "export") page.say(result.copied ? "Public key copied. Paste it into a message or share it with anyone who wants to write to you securely." : "Saved to " + result.path + ".")
            else if (action === "delete") page.say("Key deleted.")
            else if (action === "expiry") page.say(result.expires_at ? "The key now expires " + page.when(result.expires_at) + ". Send your updated public key to the people who use it." : "The key no longer expires. Send your updated public key to the people who use it.")
            else if (action === "account") page.say(result.pgp ? "Saved." : "OpenPGP is off for " + page.accountName(result.account) + ".")
            else if (action === "import") {
                const parts = []
                if ((result.added || []).length) parts.push((result.added.length === 1 ? "1 key" : result.added.length + " keys") + " added")
                if ((result.updated || []).length) parts.push((result.updated.length === 1 ? "1 key" : result.updated.length + " keys") + " updated")
                if ((result.revoked || []).length) parts.push((result.revoked.length === 1 ? "1 key" : result.revoked.length + " keys") + " marked revoked")
                page.say(parts.join(", ") + ".")
            }
        }
    }
    FileDialog {
        id: saveDialog
        objectName: "pgpSaveDialog"
        fileMode: FileDialog.SaveFile
        defaultSuffix: "asc"
        nameFilters: ["OpenPGP key (*.asc)", "All files (*)"]
        onAccepted: {
            const save = page.pendingSave; page.pendingSave = null
            if (save) page.keyring.exportKey(save.fingerprint, save.kind, save.passphrase, selectedFile)
        }
        onRejected: page.pendingSave = null
    }
    PgpKeyDialog { id: createDialog; onCreated: function(key) { page.say("Key created. Back it up now, so you can still read your encrypted mail if this computer is lost."); backupDialog.show(key) } }
    PgpImportDialog { id: importDialog }
    PgpBackupDialog { id: backupDialog; onChosen: function(key, passphrase) { page.saveKey(key, "secret", passphrase) } }
    FmDialog {
        id: expiryDialog
        objectName: "pgpExpiryDialog"
        property var key: ({})
        property int choice: 2
        readonly property var days: [365, 730, 1095, 1825, 0]
        title: "Change expiry"
        width: Math.min(460, parent ? parent.width - 48 : 460)
        function show(key) { expiryDialog.key = key; choice = 2; open() }
        contentItem: ColumnLayout {
            spacing: 12
            Text { Layout.fillWidth: true; wrapMode: Text.Wrap; text: "From today. People who already have your public key need the updated one; save it again and send it to them."; color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmSegmented { objectName: "pgpExpiryChoice"; model: ["1 year", "2 years", "3 years", "5 years", "Never"]; currentIndex: expiryDialog.choice; onActivated: function(index) { expiryDialog.choice = index } }
        }
        footer: FmDialogFooter {
            FmDialogFooter.Button { text: "Cancel"; onClicked: expiryDialog.close() }
            FmDialogFooter.Button { objectName: "pgpExpirySave"; primary: true; text: "Save"; onClicked: { page.keyring.setExpiry(expiryDialog.key.fingerprint, expiryDialog.days[expiryDialog.choice]); expiryDialog.close() } }
        }
    }
    FmConfirmDialog {
        id: deleteDialog
        property var key: ({})
        destructive: true
        title: key.secret ? "Delete your key?" : "Delete this key?"
        actionText: "Delete"
        actionObjectName: "pgpDeleteConfirm"
        message: key.secret
            ? "Mail encrypted to this key can't be read without it, and accounts using it stop signing. Back it up first if you might need it. This can't be undone."
            : "You won't be able to encrypt to " + ((key.user_ids || [])[0] || "this person") + " or check their signatures until you import their key again."
        onAccepted: page.keyring.remove(key.fingerprint)
    }

    // One key: who it is, its fingerprint, algorithm and dates, and a menu.
    component KeyRow: ColumnLayout {
        id: keyRow
        // The key's fingerprint.
        required property var modelData
        readonly property var key: (page.keyring ? page.keyring.keys : []).find(k => k.fingerprint === modelData) || ({})
        Layout.fillWidth: true; spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.topMargin: 12; Layout.bottomMargin: 12; spacing: 14
            FmIcon { name: keyRow.key.secret ? "key" : "person"; size: 14; color: keyRow.key.status === "valid" ? Theme.inkAlpha(.6) : Theme.warn; Layout.alignment: Qt.AlignTop; Layout.topMargin: 2 }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 4
                Text {
                    objectName: "pgpKey_" + keyRow.key.fingerprint
                    text: (keyRow.key.user_ids || [])[0] || "(no name)"
                    Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                    color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13)
                }
                Text {
                    text: page.keyring.grouped(keyRow.key.fingerprint)
                    Layout.fillWidth: true; elide: Text.ElideRight
                    color: Theme.inkAlpha(.55); font.family: page.mono; font.pointSize: Theme.fontPoints(11)
                    HoverHandler { id: fingerprintHover }
                    FmToolTip { shown: fingerprintHover.hovered; text: "Fingerprint. Compare it with its owner's, in person or on a call, before trusting the key." }
                }
                Text {
                    readonly property var used: (keyRow.key.accounts || []).map(page.accountName)
                    text: [keyRow.key.algorithm, "Created " + page.when(keyRow.key.created_at), page.expiry(keyRow.key)].concat(used.length ? ["Used by " + used.join(", ")] : []).join(" · ")
                    Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    color: page.statusTone(keyRow.key); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
                }
            }
            FmButton {
                id: keyMenuButton
                objectName: "pgpKeyMenu_" + keyRow.key.fingerprint
                iconName: "more"; tip: "Export, renew or delete"
                implicitWidth: 30; implicitHeight: 30; leftPadding: 0; rightPadding: 0
                Layout.alignment: Qt.AlignTop
                onClicked: { const point = mapToItem(Overlay.overlay, width, height); keyMenu.x = point.x - keyMenu.width; keyMenu.y = point.y + 6; keyMenu.open() }
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        FmMenu {
            id: keyMenu; parent: Overlay.overlay
            contentItem: ColumnLayout {
                spacing: 0
                FmMenuItem { objectName: "pgpCopyPublic_" + keyRow.key.fingerprint; text: "Copy public key"; onClicked: { keyMenu.close(); page.keyring.exportKey(keyRow.key.fingerprint, "public", "", "") } }
                FmMenuItem { objectName: "pgpSavePublic_" + keyRow.key.fingerprint; text: "Save public key…"; onClicked: { keyMenu.close(); page.saveKey(keyRow.key, "public") } }
                FmMenuItem { objectName: "pgpBackup_" + keyRow.key.fingerprint; visible: keyRow.key.secret; separatorAbove: true; text: "Back up secret key…"; onClicked: { keyMenu.close(); backupDialog.show(keyRow.key) } }
                FmMenuItem { objectName: "pgpRevocation_" + keyRow.key.fingerprint; visible: keyRow.key.secret; text: "Save revocation certificate…"; onClicked: { keyMenu.close(); page.saveKey(keyRow.key, "revocation") } }
                FmMenuItem { objectName: "pgpExpiry_" + keyRow.key.fingerprint; visible: keyRow.key.secret && keyRow.key.status !== "revoked"; text: "Change expiry…"; onClicked: { keyMenu.close(); expiryDialog.show(keyRow.key) } }
                FmMenuItem { objectName: "pgpDelete_" + keyRow.key.fingerprint; text: "Delete…"; separatorAbove: true; danger: true; onClicked: { keyMenu.close(); deleteDialog.key = keyRow.key; deleteDialog.open() } }
            }
        }
    }
    component SectionTitle: Text { Layout.topMargin: 28; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(14); font.weight: Font.Medium }
    component Note: Text { Layout.fillWidth: true; Layout.maximumWidth: 640; Layout.topMargin: 6; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); lineHeight: 1.4 }

    Note {
        Layout.topMargin: 0
        text: "Sign your mail so people can check it's from you, and encrypt it so only the people you write to can read it. Keys are OpenPGP, so they work with GnuPG, Thunderbird, Proton, Mailvelope and other OpenPGP apps."
    }
    RowLayout {
        Layout.topMargin: 16; spacing: 8
        FmButton { objectName: "pgpCreateKey"; text: "Create key…"; iconName: "plus"; outlined: true; enabled: backend.connected && !!page.keyring && page.keyring.busy === ""; onClicked: createDialog.show(page.accounts.find(a => !(page.keyring.accounts || {})[a.id]) || page.accounts[0] || null) }
        FmButton { objectName: "pgpImportKey"; text: "Import key…"; iconName: "download"; outlined: true; enabled: backend.connected && !!page.keyring && page.keyring.busy === ""; onClicked: importDialog.show() }
        Text { visible: !!page.keyring && page.keyring.busy !== ""; text: ({generate: "Creating key…", import: "Importing…", export: "Exporting…", expiry: "Renewing…", delete: "Deleting…"})[page.keyring ? page.keyring.busy : ""] || ""; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
    }
    Text {
        objectName: "encryptionStatus"
        visible: page.status.length > 0
        text: page.status
        Layout.fillWidth: true; Layout.maximumWidth: 640; Layout.topMargin: 10; wrapMode: Text.Wrap; textFormat: Text.PlainText
        color: page.statusError ? Theme.error : Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        Accessible.role: Accessible.AlertMessage
    }

    SectionTitle { text: "Your keys" }
    Rectangle { Layout.fillWidth: true; Layout.topMargin: 10; implicitHeight: 1; color: Theme.line }
    Repeater { model: page.ownFingerprints; KeyRow {} }
    Note {
        objectName: "pgpNoOwnKeys"
        visible: !!page.keyring && !page.keyring.loading && page.ownKeys.length === 0
        Layout.topMargin: 12
        text: "No keys yet. Create one for an account, or import a secret key you already use elsewhere."
    }

    SectionTitle { text: "Accounts" }
    Note { text: "The key each account signs and decrypts with, and what new mail does by default. Encrypt turns on only when every recipient has a key here. You can change both in each message." }
    Rectangle { Layout.fillWidth: true; Layout.topMargin: 10; implicitHeight: 1; color: Theme.line }
    Repeater {
        model: page.accountIds
        ColumnLayout {
            id: accountRow
            // The account's id.
            required property var modelData
            readonly property var account: page.accounts.find(a => a.id === modelData) || ({})
            readonly property var choice: (page.keyring ? page.keyring.accounts : {})[modelData] || null
            readonly property var usable: page.usableFor(account)
            readonly property var options: [null].concat(usable)
            // A default just switched shows at once, until the saved choice
            // comes back or the save fails.
            property var pendingSign: undefined
            property var pendingEncrypt: undefined
            readonly property bool sign: pendingSign !== undefined ? pendingSign : !!choice && choice.sign_by_default
            readonly property bool encrypt: pendingEncrypt !== undefined ? pendingEncrypt : !!choice && choice.encrypt_by_default
            onChoiceChanged: if (!choice || (choice.sign_by_default === sign && choice.encrypt_by_default === encrypt)) { pendingSign = undefined; pendingEncrypt = undefined }
            function save(sign, encrypt) {
                pendingSign = sign; pendingEncrypt = encrypt
                page.keyring.useForAccount(modelData, choice.fingerprint, sign, encrypt)
            }
            Connections { target: page; function onSaveFailed() { accountRow.pendingSign = undefined; accountRow.pendingEncrypt = undefined } }
            Layout.fillWidth: true; spacing: 0
            RowLayout {
                Layout.fillWidth: true; Layout.topMargin: 10; Layout.bottomMargin: 10; spacing: 14
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 2
                    Text { text: accountRow.account.name; Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
                    Text { text: accountRow.account.email; Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText; color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                }
                FmComboBox {
                    objectName: "pgpAccountKey_" + accountRow.modelData
                    implicitWidth: 220; implicitHeight: 32
                    enabled: backend.connected && page.keyring && page.keyring.busy === ""
                    model: accountRow.options.map(k => k ? (k.key_id.slice(8).match(/.{4}/g).join(" ") + " · " + k.algorithm) : (accountRow.usable.length ? "Off" : "Off · no key for this address"))
                    currentIndex: { const at = accountRow.options.findIndex(k => k && accountRow.choice && k.fingerprint === accountRow.choice.fingerprint); return at < 0 ? 0 : at }
                    onActivated: function(index) {
                        const key = accountRow.options[index]
                        page.keyring.useForAccount(accountRow.modelData, key ? key.fingerprint : "", accountRow.choice ? accountRow.sign : true, accountRow.choice ? accountRow.encrypt : false)
                    }
                    Accessible.name: "OpenPGP key for " + accountRow.account.email
                }
                RowLayout {
                    spacing: 6; opacity: accountRow.choice ? 1 : .35
                    // The switches show the row's value; a click saves the other one.
                    // They stay enabled while saving, so nothing dims or jumps.
                    FmSwitch {
                        objectName: "pgpSignDefault_" + accountRow.modelData; pill: true; checkable: false
                        enabled: !!accountRow.choice
                        checked: accountRow.sign
                        onClicked: accountRow.save(!accountRow.sign, accountRow.encrypt)
                        Accessible.name: "Sign new mail"
                    }
                    Text { text: "Sign"; color: Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                    FmSwitch {
                        objectName: "pgpEncryptDefault_" + accountRow.modelData; pill: true; checkable: false; Layout.leftMargin: 10
                        enabled: !!accountRow.choice
                        checked: accountRow.encrypt
                        onClicked: accountRow.save(accountRow.sign, !accountRow.encrypt)
                        Accessible.name: "Encrypt new mail when every recipient has a key"
                    }
                    Text { text: "Encrypt"; color: Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        }
    }

    SectionTitle { text: "Other people's keys" }
    Note { text: "Import someone's public key to encrypt mail to them and check their signatures. A key is only as trustworthy as where you got it: compare its fingerprint with its owner." }
    Rectangle { Layout.fillWidth: true; Layout.topMargin: 10; implicitHeight: 1; color: Theme.line }
    Repeater { model: page.otherFingerprints; KeyRow {} }
    Note { visible: !!page.keyring && !page.keyring.loading && page.otherKeys.length === 0; Layout.topMargin: 12; text: "None yet." }

    Note {
        Layout.topMargin: 28
        color: Theme.inkAlpha(.45); font.pointSize: Theme.fontPoints(11.5)
        text: backend.syntheticProfile
            ? "This demo profile keeps the key that protects secret keys in a file in the profile. Use a live profile for real keys."
            : "Secret keys stay on this computer, sealed with a key kept in your system keychain, and are never sent to Google. Encrypted mail is decrypted only while you read it; its text isn't searchable or summarized, and drafts you'll encrypt aren't saved to Gmail Drafts. Subjects aren't encrypted, so Gmail can keep replies together."
    }
}
