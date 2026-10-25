import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// "Import key?": what a key file attached to a message holds, read before
// anything is stored. It shows each key's user IDs and fingerprint, says
// when a key isn't for the person who sent it, and when it would take an
// address another key (or one of your accounts) has. Only public keys
// import from mail.
FmDialog {
    id: dialog
    objectName: "pgpKeyPreviewDialog"
    // Answers the import: the engine's result, or {code, message} when `ok` is false.
    signal imported(var result, bool ok)
    property string account: ""
    property string attachmentId: ""
    property string sender: ""
    property string ticket: ""
    property string importTicket: ""
    property var preview: null
    property string error: ""
    readonly property var keys: preview ? preview.keys || [] : []
    readonly property var conflicts: preview ? preview.conflicts || [] : []
    readonly property bool working: ticket.length > 0 || importTicket.length > 0
    title: "Import key?"
    width: Math.min(560, parent ? parent.width - 48 : 560)
    closePolicy: importTicket.length ? Popup.NoAutoClose : Popup.CloseOnEscape | Popup.CloseOnPressOutside
    function show(accountId, attachment, from) {
        account = accountId; attachmentId = attachment; sender = String(from || "").toLowerCase()
        preview = null; error = ""; importTicket = ""
        ticket = backend.pgpKeys.previewAttachment(account, attachmentId)
        open()
    }
    // A preview still on its way is dropped; an import can't be closed on.
    onClosed: ticket = ""
    function grouped(fingerprint) { return backend.pgpKeys.grouped(fingerprint) }
    function holder(fingerprint) { const key = backend.pgpKeys.key(fingerprint); return (key.user_ids || [])[0] || grouped(fingerprint) }
    function conflictText(conflict) {
        if (conflict.own) return conflict.address + " is one of your own addresses."
        return conflict.address + " already has a key: " + conflict.holders.map(dialog.holder).join(", ")
            + ". Importing this one makes it the key used to encrypt to " + conflict.address + "."
    }
    Connections {
        // Connected only while a preview or import is on its way, so a dialog
        // that never opens doesn't start the keyring.
        target: dialog.working ? backend.pgpKeys : null
        function onPreviewReady(ticket, result, ok) {
            if (ticket !== dialog.ticket) return
            dialog.ticket = ""
            if (ok) dialog.preview = result
            else dialog.error = result.message || "This key couldn't be read."
        }
        function onFinished(ticket, action, result, ok) {
            if (ticket !== dialog.importTicket) return
            dialog.importTicket = ""
            dialog.imported(result, ok)
            if (ok) dialog.close()
            else dialog.error = result.message || "Nothing was imported."
        }
    }
    contentItem: ColumnLayout {
        spacing: 12
        Text {
            objectName: "pgpKeyPreviewIntro"
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            visible: !!dialog.preview
            text: "Anyone can make a key with any name and address. Before relying on it, check its fingerprint with "
                + (dialog.sender || "its owner") + " some other way, such as in person or by phone."
            color: Theme.inkAlpha(.6); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        }
        Text {
            visible: dialog.ticket.length > 0
            text: "Reading the key…"; textFormat: Text.PlainText
            color: Theme.inkAlpha(.55); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        }
        Repeater {
            model: dialog.keys
            ColumnLayout {
                id: keyCard
                required property var modelData
                required property int index
                readonly property bool forSender: !dialog.sender || (modelData.addresses || []).indexOf(dialog.sender) >= 0
                objectName: "pgpKeyPreview_" + index
                Layout.fillWidth: true; spacing: 4
                Text {
                    Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    text: (keyCard.modelData.user_ids || []).join("\n") || "A key with no user ID"
                    color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); font.weight: Font.Medium
                }
                Text {
                    objectName: "pgpKeyPreviewFingerprint_" + keyCard.index
                    Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    text: dialog.grouped(keyCard.modelData.fingerprint || "")
                    color: Theme.inkAlpha(.75); font.family: backend.fixedFontFamily(); font.pointSize: Theme.fontPoints(11.5)
                }
                Text {
                    Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    text: [keyCard.modelData.algorithm, "made " + Qt.formatDate(new Date((keyCard.modelData.created_at || 0) * 1000), "d MMM yyyy"),
                           keyCard.modelData.action === "update" ? "updates a key you have" : "new to your keyring",
                           keyCard.modelData.status !== "valid" ? keyCard.modelData.status : ""].filter(function(part) { return !!part }).join(" · ")
                    color: Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
                }
                Text {
                    objectName: "pgpKeyPreviewNotSender_" + keyCard.index
                    visible: !keyCard.forSender
                    Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
                    text: "This key isn't for " + dialog.sender + ", who sent it."
                    color: Theme.warn; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
                }
            }
        }
        Text {
            visible: !!dialog.preview && (dialog.preview.revoked || []).length > 0
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: "It revokes " + ((dialog.preview || {}).revoked || []).map(dialog.holder).join(", ") + "."
            color: Theme.inkAlpha(.7); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
        }
        Text {
            objectName: "pgpKeyPreviewConflicts"
            visible: dialog.conflicts.length > 0
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: dialog.conflicts.map(dialog.conflictText).join("\n")
            color: Theme.warn; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
        }
        Text { objectName: "pgpKeyPreviewError"; visible: dialog.error.length > 0; text: dialog.error; Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.error; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
    }
    footer: FmDialogFooter {
        FmDialogFooter.Button { text: dialog.preview ? "Cancel" : "Close"; enabled: !dialog.importTicket.length; onClicked: dialog.close() }
        FmDialogFooter.Button {
            objectName: "pgpKeyPreviewImport"; primary: true; visible: !!dialog.preview
            text: dialog.importTicket.length ? "Importing…" : dialog.conflicts.length ? "Import anyway" : "Import"
            enabled: backend.connected && !dialog.working
            onClicked: { dialog.error = ""; dialog.importTicket = backend.pgpKeys.importPreviewed(dialog.preview.path, dialog.conflicts.length > 0) }
        }
    }
}
