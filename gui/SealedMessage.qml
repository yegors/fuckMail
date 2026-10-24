// An encrypted message in MailView before it is read: a label in the style of
// a forward's, redaction bars where the text would be, and Decrypt (mail
// received) or Reveal (mail the person sent). Also explains why it can't be
// decrypted here. MessageSource owns the state; nothing is decrypted until
// the person asks.
import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: sealed
    required property var source
    readonly property var message: source.message
    readonly property bool sent: !!message.sent
    readonly property bool problem: source.undecryptable
    readonly property bool clickable: source.canDecrypt && !source.decrypting && !problem
    objectName: "sealedMessage_" + (message.id || "")
    spacing: 12
    function keyIds(ids) { return (ids || []).map(function(id) { return String(id).slice(-8).replace(/(.{4})(?=.)/g, "$1 ") }).join(", ") }
    readonly property string hint: {
        const details = source.pgp && source.pgp.decryption ? source.pgp.decryption : ({})
        if (source.decryption === "no_key") return "Encrypted to " + (keyIds(details.recipients) ? "key " + keyIds(details.recipients) + ", which isn't here" : "a key that isn't here") + ". Import its secret key in Settings → Encryption."
        if (source.decryption === "unavailable") return "Your secret keys couldn't be opened: " + (details.message || "the system keychain refused.")
        if (source.decryption === "failed") return "It can't be read: " + (details.message || "it's damaged.")
        if (source.decryption === "sealed" && !source.canDecrypt) return "Encrypted to " + (keyIds(details.recipients) ? "key " + keyIds(details.recipients) + ", which isn't here" : "a key that isn't here") + ". Import its secret key in Settings → Encryption."
        if (sent) return message.is_outgoing ? "The copy it was sent from stays on this computer." : "Your copy is encrypted to your key too."
        return "Decrypted in memory only while you read it; the text isn't saved or searchable."
    }
    // Bar widths that differ between messages but never change for one; a
    // message on its way out has as many lines as its text, up to five.
    readonly property var bars: {
        let seed = 7
        const id = String(message.id || "")
        for (let i = 0; i < id.length; ++i) seed = (seed * 31 + id.charCodeAt(i)) % 104729
        const count = message.is_outgoing ? Math.max(1, Math.min(5, Math.ceil(String(message.text || "").trim().length / 64))) : 3
        const out = []
        for (let line = 0; line < count; ++line) {
            seed = (seed * 48271) % 2147483647
            const spread = Math.floor(seed / 97) % 25
            let width = line === count - 1 && count > 1 ? .34 + spread / 100 : .72 + spread / 100
            // Neighbouring lines never look the same length.
            if (line > 0 && Math.abs(width - out[line - 1]) < .05) width += width > .84 ? -.09 : .09
            out.push(width)
        }
        return out
    }

    RowLayout {
        spacing: 7
        FmIcon { name: "lock"; size: 11; color: sealed.problem ? Theme.warn : Theme.inkAlpha(.5) }
        Text {
            objectName: "sealedMessageLabel_" + (sealed.message.id || "")
            text: sealed.sent ? "Encrypted · only you and the recipients can read it" : "Encrypted message"
            color: Theme.inkAlpha(.5); textFormat: Text.PlainText
            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); font.capitalization: Font.AllUppercase; font.letterSpacing: 0.6
        }
    }
    // The redacted text. A click decrypts or reveals, like the button.
    Item {
        id: redaction
        objectName: "sealedMessageBars_" + (sealed.message.id || "")
        Layout.fillWidth: true; Layout.maximumWidth: 640
        implicitHeight: barColumn.implicitHeight
        Column {
            id: barColumn
            width: parent.width; spacing: 11
            Repeater {
                model: sealed.bars
                Rectangle {
                    required property real modelData
                    width: Math.round(barColumn.width * modelData); height: 9; radius: 4.5
                    color: sealed.problem ? Theme.withAlpha(Theme.warn, .12) : Theme.inkAlpha(redactionHover.hovered && sealed.clickable ? .14 : .085)
                    Behavior on color { ColorAnimation { duration: Theme.ms(140); easing.type: Theme.enterEasing } }
                }
            }
        }
        // While decrypting, the bars breathe.
        SequentialAnimation on opacity {
            running: sealed.source.decrypting; loops: Animation.Infinite
            alwaysRunToEnd: true
            NumberAnimation { to: .45; duration: Theme.ms(520); easing.type: Easing.InOutSine }
            NumberAnimation { to: 1; duration: Theme.ms(520); easing.type: Easing.InOutSine }
        }
        HoverHandler { id: redactionHover; cursorShape: sealed.clickable ? Qt.PointingHandCursor : Qt.ArrowCursor }
        TapHandler { enabled: sealed.clickable; onTapped: sealed.source.decrypt() }
    }
    RowLayout {
        Layout.fillWidth: true; Layout.topMargin: 2; spacing: 12
        FmButton {
            objectName: "sealedMessageDecrypt_" + (sealed.message.id || "")
            visible: !sealed.problem && (sealed.source.canDecrypt || sealed.source.decryption === "")
            text: sealed.source.decrypting ? (sealed.sent ? "Revealing…" : "Decrypting…") : sealed.sent ? "Reveal" : "Decrypt"
            iconName: sealed.sent ? "" : "key"; iconSize: 13
            prominent: !sealed.sent; outlined: sealed.sent
            enabled: sealed.clickable
            implicitHeight: 30; leftPadding: 14; rightPadding: 14
            font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium
            tip: sealed.sent ? "Show what you sent" : "Decrypt with your OpenPGP key"
            onClicked: sealed.source.decrypt()
        }
        Text {
            objectName: "sealedMessageHint_" + (sealed.message.id || "")
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: sealed.hint
            color: sealed.problem ? Theme.warn : Theme.inkAlpha(.45)
            font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
        }
    }
}
