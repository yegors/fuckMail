// Per-message controller in MailView: loads the parsed source (HTML body and
// header metadata) through backend.requestMessageSource() for the reader, and
// owns the "Message source" dialog that shows the raw message.
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: source; objectName: "messageSourceController_" + message.id
    property var message: ({})
    property string account: backend.account
    property bool active: false
    property string ticket: ""
    property var result: ({})
    property string error: ""
    // Raw text is read for the dialog only and released when it closes.
    property string raw: ""
    property string rawTicket: ""
    // Mail shows as plain text unless its sender is in this account's
    // html_senders; choosing HTML or text here applies to all their mail.
    readonly property string sender: String(message.sender_email || "").trim().toLowerCase()
    readonly property bool canChoose: !!account && sender.indexOf("@") > 0
    readonly property bool plain: !canChoose || !(backend.preferences.html_senders || []).some(function(entry) { return entry.account === account && entry.email === sender })
    function setPlain(value) {
        if (!canChoose || value === plain) return
        const list = (backend.preferences.html_senders || []).filter(function(entry) { return !(entry.account === account && entry.email === sender) })
        if (!value) {
            // The engine keeps at most 2,000; the oldest choice makes room.
            while (list.length >= 2000) list.shift()
            list.push({account: account, email: sender})
        }
        backend.setPreferences({html_senders: list})
    }
    readonly property string identity: account + "/" + (message.id || "") + "/" + (message.source_version || "") + "/" + unsealed
    readonly property bool hasHtml: !!result.body_html && !result.html_truncated
    readonly property var metadata: (result.metadata || []).filter(function(row) { return row.people.length > 0 })
    // Gmail's verdicts on who sent received mail: from the parsed source, or
    // carried by a message read live from Gmail.
    readonly property var senderCheck: message.sent ? null : result.sender_check || message.sender_check || null
    readonly property bool senderVerified: !!senderCheck && senderCheck.verified === true
    readonly property var senderWarnings: senderCheck ? senderCheck.warnings || [] : []
    // OpenPGP: how the message is protected and what opening it found.
    readonly property var pgp: result.pgp || null
    // The message first: reading it can adopt a prepared source, which sets `result`.
    readonly property bool encrypted: message.pgp === "encrypted" || (!!pgp && pgp.encrypted === true)
    readonly property string decryption: pgp && pgp.decryption ? pgp.decryption.state || "" : ""
    // Encrypted mail stays sealed until the person decrypts it, or reveals
    // what they sent; the choice lasts until the app closes. A message still
    // on its way out reveals the copy it was sent from.
    readonly property bool unsealed: backend.unsealedMessages.indexOf(account + "\n" + (message.id || "")) >= 0
    readonly property bool sealed: encrypted && (!unsealed || (!message.is_outgoing && (decryption === "" || decryption === "sealed")))
    // Decrypting was tried and failed: no key here, the keychain refused, or damage.
    readonly property bool unreadable: encrypted && ["no_key", "unavailable", "failed"].indexOf(decryption) >= 0
    // Decrypting has been asked for and its reading hasn't come back.
    readonly property bool decrypting: sealed && unsealed && !error
    // Sealed mail names one of this installation's keys among its recipients.
    readonly property bool canDecrypt: !!message.is_outgoing || (decryption === "sealed" && !!pgp.decryption.can_decrypt)
    // It failed, or no key here can open it.
    readonly property bool undecryptable: unreadable || (decryption === "sealed" && !canDecrypt)
    function decrypt() { if (encrypted) backend.setUnsealed(message.id, true) }
    function seal() { backend.setUnsealed(message.id, false) }
    readonly property var signatures: pgp ? pgp.signatures || [] : []
    // The best signature result: good and from the sender beats good, beats the rest.
    readonly property var signature: signatures.length === 0 ? null
        : signatures.find(function(s) { return s.status === "good" && s.sender_matches }) || signatures.find(function(s) { return s.status === "good" }) || signatures[0]
    readonly property bool trustedSignature: !!signature && signature.status === "good" && signature.sender_matches
    // Files inside an encrypted message, once it is decrypted.
    readonly property var decryptedAttachments: pgp && pgp.attachments ? pgp.attachments : null
    function keyId(id) { return String(id || "").slice(-16).replace(/(.{4})(?=.)/g, "$1 ") }
    // The header icon's tooltip: one line for encryption, one for the signature.
    readonly property string pgpSummary: {
        const lines = []
        if (encrypted) lines.push(decryption === "decrypted" ? "Encrypted · decrypted with your key" : unreadable ? "Encrypted · can't be read here" : "Encrypted")
        if (signature) {
            const who = signature.signer || ("key " + keyId(signature.key_id))
            lines.push(({good: signature.sender_matches ? "Signed by " + who : "Signed by " + who + ", which isn't the sender's address",
                bad: "Signature doesn't match: the message changed after it was signed",
                unknown_key: "Signed with key " + keyId(signature.key_id) + ", which isn't in your keyring",
                key_expired: "Signed by " + who + " with a key that had expired",
                key_revoked: "Signed by " + who + " with a key that has been revoked",
                weak: "Signed by " + who + " with a hash too weak to trust",
                invalid_key: "Signed by " + who + " with a key that can't sign"})[signature.status] || "Signed")
            if (signature.fingerprint) lines.push(signature.fingerprint.replace(/(.{4})(?=.)/g, "$1 "))
        }
        return lines.join("\n")
    }
    readonly property bool popupOpen: original.visible
    // A prepared or held reading made before the person decrypted (or sealed
    // again) doesn't count.
    function matchesSeal(reading) {
        const state = reading.pgp && reading.pgp.decryption ? reading.pgp.decryption.state || "" : ""
        return unsealed ? state !== "sealed" : state !== "decrypted"
    }
    function adopt() {
        const prepared = message.reader_source
        if (prepared && prepared.id === message.id && prepared.account === account && prepared.source_version === message.source_version && matchesSeal(prepared)) {
            result = prepared; error = ""; return true
        }
        if (message.reader_error) { error = message.reader_error; return true }
        return false
    }
    onIdentityChanged: { ticket = ""; result = ({}); error = ""; raw = ""; rawTicket = ""; original.close(); adopt(); deferredLoad.restart() }
    onMessageChanged: adopt()
    onActiveChanged: if (active) load()
    Component.onCompleted: { adopt(); load() }
    Timer {id:deferredLoad;interval:0;onTriggered:source.load()}
    function load() {
        if (!active || !backend.connected || !message.source_available || !message.source_version || ticket || result.id || error) return
        ticket = backend.requestMessageSource(message.id, message.source_version)
    }
    function loadRaw() {
        if (raw || rawTicket || !backend.connected || !message.source_available || !message.source_version) return
        rawTicket = backend.requestMessageSource(message.id, message.source_version, true)
    }
    function releaseRaw() { if (!original.visible) { raw = ""; rawTicket = "" } }
    function show() { if (error) error = ""; load(); loadRaw(); original.open() }
    Connections {
        target: backend
        function onConnectedChanged() { if (!backend.connected) { ticket = ""; rawTicket = "" } else source.load() }
        function onMessageSourceReady(request, content, ok) {
            if (request === source.rawTicket) {
                source.rawTicket = ""
                if (ok && content.id === source.message.id && content.account === source.account && content.source_version === source.message.source_version) source.raw = content.raw || ""
                else if (original.visible) source.error = content.message || "Message source is unavailable."
                return
            }
            if (request !== source.ticket) return
            source.ticket = ""
            if (ok && content.id === source.message.id && content.account === source.account && content.source_version === source.message.source_version) source.result = content
            else source.error = content.message || "Message source is unavailable."
        }
    }
    FmDialog {
        id: original; objectName: "messageSourceDialog_" + source.message.id
        width: Math.min(800, parent.width - 48); height: Math.min(640, parent.height - 48)
        title: "Message source"; standardButtons: Dialog.Close
        onClosed: source.releaseRaw()
        contentItem: ColumnLayout {
            spacing: 12; property bool blocksWorkspaceShortcuts: true
            RowLayout {
                Layout.fillWidth: true
                Text { text: Theme.oneLine(source.message.subject); maximumLineCount: 1; Layout.fillWidth: true; elide: Text.ElideRight; color: Theme.secondary; font.pointSize: Theme.fontPoints(12); textFormat: Text.PlainText }
                FmButton { text: "Copy"; outlined: true; implicitHeight: 28; enabled: !!source.raw; onClicked: backend.copyText(source.raw) }
            }
            Text { visible: !!source.error || !!source.result.raw_truncated; text: source.error || "This source preview is shortened. The full message remains stored locally."; color: source.error ? Theme.error : Theme.muted; font.pointSize: Theme.fontPoints(11.5); wrapMode: Text.Wrap; Layout.fillWidth: true }
            ScrollView {
                ScrollBar.vertical: FmScrollBar {}
                Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                TextArea { objectName: "messageSourceText_" + source.message.id; text: original.visible ? (source.raw || (source.ticket || source.rawTicket ? "Loading source…" : "Source unavailable.")) : ""; textFormat: TextEdit.PlainText; readOnly: true; selectByMouse: true; wrapMode: TextEdit.WrapAnywhere; font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(11.5); color: Theme.secondary; background: Rectangle { color: Theme.canvas; radius: 5 } ContextMenu.menu: null; TextMenuHandler {} }
            }
        }
    }
}
