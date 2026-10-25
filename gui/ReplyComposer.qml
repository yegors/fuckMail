import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import QtQuick.Shapes

// The composer sits inline in the reading pane. A new message fills at least
// max(420px, half the window) and its body grows; replies start smaller. The
// footer holds Send, Attach, one status line and Discard. Drafts sync with
// Gmail silently; only a real problem is shown.
Item {
    id: root
    signal dismissed()
    // Recent mail with the first recipient opens its thread; the message stays in Drafts.
    signal recentOpened(string conversationId)
    signal allMailWith(string email)
    // A recipient chip asks for the contact card; MailView owns it.
    signal contactHovered(string email, string name, Item anchor)
    signal contactLeft(Item anchor)
    signal contactShown(string email, string name, Item anchor)
    property bool loading: false
    property bool ccOpen: false
    property bool bccOpen: false
    // A new message fills this much of the pane; 0 for replies.
    property real minimumHeight: 0
    // The height the form can show in; its body stops growing where the
    // whole form, footer included, would no longer fit. 0 sets no limit.
    property real availableHeight: 0
    // Everything but the body: the title, fields and footer. Measured once a
    // layout settles; reading it in the body's own size would loop the layout.
    property real chromeHeight: 0
    function measureChrome() { chromeHeight = contents.height - composerScroll.height }
    readonly property bool popupOpen: fromMenu.opened || toField.popupOpen || ccField.popupOpen || bccField.popupOpen || discardDialog.opened || reloadDialog.opened || conflictDialog.opened || contextMenu.opened || localAttachmentDialog.visible
    readonly property string mode: backend.draft.mode || "reply"
    readonly property bool readOnly: backend.draftReadOnly || backend.accountArchived || backend.unsavedAccountBuffer
    readonly property string draftTitle: backend.unsavedAccountBuffer ? "Retained unsaved edits" : backend.accountArchived ? "Archived draft" : backend.draft.transferred ? "Moved draft" : ({new:"New message",reply:"Reply",reply_all:"Reply all",forward:"Forward"})[mode] || "Reply"
    readonly property bool hasSelection: bodyEditor.selectionEnd > bodyEditor.selectionStart
    readonly property var attachmentIds: backend.draft.attachment_ids || []
    readonly property var availableAttachments: {
        let items = []
        for (const message of (backend.conversation.messages || []))
            for (const item of (message.attachments || [])) if (item.state === "available") items.push(item)
        return items
    }
    // OpenPGP, once the account has a key or the draft already asks for it.
    // Local demo captures are never signed or encrypted, so they don't offer it.
    readonly property bool pgpSign: backend.draft.pgp_sign === true
    readonly property bool pgpEncrypt: backend.draft.pgp_encrypt === true
    readonly property bool pgpShown: !root.readOnly && !backend.syntheticProfile && (!!backend.currentAccount.pgp || pgpSign || pgpEncrypt)
    // Recipients without a usable key, and which recipients that answer
    // covers. Mail can be encrypted once every recipient has a key.
    property var pgpMissing: []
    property string pgpTicket: ""
    property string pgpAsked: ""
    property string pgpChecked: ""
    readonly property string pgpRecipients: [].concat(backend.draft.to_recipients || [], backend.draft.cc_recipients || [], backend.draft.bcc_recipients || [])
        .map(function(r) { return String(r.email || "").toLowerCase() }).filter(function(e) { return e.length }).join(",")
    readonly property bool pgpKnown: pgpChecked === pgpRecipients
    readonly property bool pgpReady: pgpRecipients.length > 0 && pgpKnown && pgpMissing.length === 0
    // Drafts whose Encrypt the person set; on the rest, encrypt by default
    // turns Encrypt on once every recipient has a key.
    property var pgpChosen: ({})
    readonly property bool pgpEncryptDefault: !!backend.currentAccount.pgp && backend.currentAccount.pgp.encrypt_by_default === true
    readonly property string pgpDraft: String(backend.draft.draft_id || "")
    function checkPgp() {
        if (!pgpShown || !pgpRecipients.length) { pgpTicket = ""; pgpMissing = []; pgpChecked = pgpRecipients; followPgp(); return }
        pgpAsked = pgpRecipients
        pgpTicket = backend.pgpKeys.checkRecipients(backend.account, pgpAsked.split(","))
    }
    // A reply not yet saved starts from the engine's defaults; changing it
    // here would save a draft just for opening the conversation.
    readonly property bool pgpSaved: Number(backend.draft.revision || 0) > 0
    onPgpSavedChanged: followPgp()
    function followPgp() {
        if (!pgpShown || !pgpKnown || !pgpSaved || root.readOnly || backend.draftLocked || !pgpDraft || pgpChosen[pgpDraft]) return
        // Encrypt never turns itself off: a recipient without a key stops the
        // send instead, so nothing goes out readable that wasn't chosen to.
        if (!pgpEncrypt && pgpReady && pgpEncryptDefault) backend.setDraftProtection(pgpSign, true)
    }
    function choosePgpEncrypt(on) {
        const chosen = Object.assign({}, pgpChosen); chosen[pgpDraft] = true; pgpChosen = chosen
        backend.setDraftProtection(pgpSign, on)
    }
    onPgpRecipientsChanged: checkPgp()
    onPgpShownChanged: checkPgp()
    onPgpDraftChanged: followPgp()
    Connections {
        // Only an account with a key reaches the keyring.
        target: root.pgpShown ? backend.pgpKeys : null
        ignoreUnknownSignals: true
        function onRecipientsReady(ticket, result, ok) {
            if (ticket !== root.pgpTicket) return
            root.pgpMissing = ok ? (result.recipients || []).filter(function(r) { return r.status !== "ready" }).map(function(r) { return r.address }) : root.pgpAsked.split(",")
            root.pgpChecked = root.pgpAsked
            root.followPgp()
        }
        function onKeyringChanged() { root.checkPgp() }
    }
    readonly property var firstRecipient: (backend.draft.to_recipients || [])[0] || null
    // Shown under a new message once its first recipient shares mail with this account.
    readonly property bool recentVisible: mode === "new" && !readOnly && !!firstRecipient && backend.recentWith.email === String(firstRecipient.email || "").toLowerCase() && (backend.recentWith.rows || []).length > 0
    readonly property string recentName: firstRecipient ? String(firstRecipient.name || firstRecipient.email || "").split(/\s+/)[0] : ""
    onFirstRecipientChanged: if (mode === "new") backend.loadRecentWith(firstRecipient ? firstRecipient.email : "")
    // A forward with no recipient yet starts in To; everything else in the body.
    function focusEditor() {
        if (mode === "forward" && !firstRecipient && !root.readOnly) toField.focusEntry()
        else bodyEditor.forceActiveFocus()
    }
    function chooseLocalAttachment() {
        if (root.readOnly) return
        localAttachmentDialog.targetAccount = backend.account
        localAttachmentDialog.targetDraft = backend.draft.draft_id || ""
        localAttachmentDialog.open()
    }
    function attachmentSize(bytes) {
        return bytes >= 1048576 ? (bytes / 1048576).toFixed(1) + " MB" : bytes >= 1024 ? Math.ceil(bytes / 1024) + " KB" : bytes + " B"
    }
    function confirmDiscard() {
        if (backend.draftLocked || backend.draftBusy || backend.attachmentImport.pending) return
        discardDialog.capturedContext = backend.draftDiscardContext()
        if (backend.draftHasContent()) discardDialog.open(); else backend.discardDraft(discardDialog.capturedContext)
    }
    function syncDraft() {
        loading = true
        document.setIdentity(backend.account + "/" + (backend.activeDraftId || backend.selectedId))
        document.load(backend.draft.body || "", backend.draft.body_html || "")
        loading = false
    }
    function dayLabel(value) {
        const date = new Date(value), today = new Date(backend.clockDay)
        if (isNaN(date.getTime())) return ""
        const days = Math.round((Date.UTC(today.getFullYear(), today.getMonth(), today.getDate()) - Date.UTC(date.getFullYear(), date.getMonth(), date.getDate())) / 86400000)
        return days === 0 ? "Today" : days === 1 ? "Yesterday" : Qt.formatDate(date, date.getFullYear() === today.getFullYear() ? "d MMM" : "d MMM yyyy")
    }
    // The writer's own words mention an attachment and nothing is attached.
    // Forwarded and quoted text does not count. Advisory only.
    readonly property bool attachmentMissing: {
        if ((backend.draft.attachments || []).length || attachmentIds.length) return false
        const own = document.plainText.split("---------- Forwarded message ----------")[0]
            .split("\n").filter(line => !/^\s*>/.test(line)).join("\n")
        return /\b(attach(ed|ment|ments|ing)?|enclosed)\b/i.test(own)
    }
    // One line under the buttons: a problem first, then what happens next.
    readonly property var hint: {
        if (backend.recipientError.length) return {text: backend.recipientError, tone: Theme.accent}
        if (backend.draftStatus.indexOf("Not saved") >= 0) return {text: backend.draftStatus, tone: Theme.error}
        if (backend.googleDraft.attention) return {text: backend.googleDraft.status || "", tone: backend.googleDraft.warning ? Theme.warn : Theme.inkAlpha(.45)}
        if (!backend.syntheticProfile && !(backend.draft.to_recipients || []).length) return {text: "Add at least one recipient", tone: Theme.inkAlpha(.45)}
        if (attachmentMissing) return {text: "Mentions an attachment — none added", tone: Theme.amber}
        if (backend.syntheticProfile) return {text: "Kept here as a draft · nothing is delivered", tone: Theme.inkAlpha(.45)}
        return {text: "Kept as a draft until it sends", tone: Theme.inkAlpha(.45)}
    }
    // An HTML forward sends the original below the author's words, as it
    // was received. It shows there in the message area, read-only, through
    // the view MailView supplies.
    readonly property bool forwardsOriginal: mode === "forward" && backend.draft.forward_html === true
    readonly property var forwardedMessage: {
        const id = backend.draft.source_message_id || ""
        for (const message of (backend.conversation.messages || [])) if (message.id === id) return message
        return null
    }
    property Component forwardedView: null
    implicitHeight: Math.max(minimumHeight, composer.implicitHeight) + (recent.visible ? recent.implicitHeight + 28 : 0)
    Accessible.name: draftTitle
    Component.onCompleted: {
        syncDraft(); if (mode === "new") backend.loadRecentWith(firstRecipient ? firstRecipient.email : "")
        for (const field of [toField, ccField, bccField]) {
            field.contactHovered.connect(root.contactHovered); field.contactLeft.connect(root.contactLeft); field.contactShown.connect(root.contactShown)
        }
    }
    Connections {
        target: backend
        // MailView closes the reply when it is discarded; Reply may reopen it.
        function onDraftChanged() { root.syncDraft() }
    }
    ReplyDocument {
        id: document
        objectName: "replyDocument"
        textDocument: bodyEditor.textDocument
        editor: bodyEditor
        readOnly: root.readOnly || backend.draftLocked
        onContentChanged: if (!root.loading) backend.editFormattedDraft(plainText, html)
        // A pasted screenshot or copied files become attachments of this draft.
        onImagePasted: image => backend.attachPastedImage(image, backend.account, backend.draft.draft_id || "")
        onFilesPasted: files => backend.attachPastedFiles(files, backend.account, backend.draft.draft_id || "")
    }
    Item {
        id: composer
        width: parent.width
        height: Math.max(root.minimumHeight, implicitHeight)
        implicitHeight: contents.implicitHeight
        Rectangle { width: 2; height: parent.height; color: fileDrop.containsDrag ? Theme.accent : Theme.accentAlpha(0.55) }
        // Send pressed: a hairline sweeps along the bottom until the sent
        // message takes the box's place, as it does on that message.
        Item {
            id: sendTrack
            objectName: "composerSendProgress"
            visible: !backend.syntheticProfile && (backend.googleSend.busy || false)
            x: 18; y: parent.height - 2; width: parent.width - 18; height: 2; clip: true; z: 1
            Rectangle { anchors.fill: parent; radius: 1; color: Theme.inkAlpha(.06) }
            Rectangle {
                height: parent.height; width: sendTrack.width * .2; radius: 1; color: Theme.accent
                NumberAnimation on x { running: sendTrack.visible; loops: Animation.Infinite; from: -sendTrack.width * .2; to: sendTrack.width; duration: 1100; easing.type: Easing.InOutQuad }
            }
        }
        DropArea {
            id: fileDrop; objectName: "draftFileDrop"
            anchors.fill: parent; z: 100
            property string targetAccount: ""
            property string targetDraft: ""
            enabled: !root.readOnly && !backend.draftLocked && backend.connected && !backend.attachmentImport.pending
            onEntered: drag => {
                drag.accepted = drag.hasUrls && drag.urls.length > 0
                targetAccount = backend.account
                targetDraft = backend.draft.draft_id || ""
            }
            onDropped: drop => {
                if (!drop.hasUrls || targetAccount !== backend.account || targetDraft !== (backend.draft.draft_id || "")) { drop.accepted = false; return }
                backend.importDraftAttachments(drop.urls, targetAccount, targetDraft)
                drop.action = Qt.CopyAction
                drop.accepted = true
            }
            Rectangle {
                anchors.fill: parent; visible: fileDrop.containsDrag
                color: Theme.surface; opacity: 0.96; border.width: 1; border.color: Theme.accent; radius: 8
                Text { anchors.centerIn: parent; text: "Drop files to attach"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(15) }
            }
        }
        ColumnLayout {
            id: contents
            anchors { fill: parent; leftMargin: 18 }
            spacing: 0
            onHeightChanged: Qt.callLater(root.measureChrome)
            // What is being written, above the form in every mode.
            RowLayout {
                objectName: "composerTitle"
                Layout.fillWidth: true; Layout.bottomMargin: 14; spacing: 10
                Rectangle {
                    implicitWidth: 28; implicitHeight: 28; radius: 5; color: Theme.withAlpha(Theme.amber, .12)
                    FmIcon { anchors.centerIn: parent; name: ({new: "edit", reply: "reply", reply_all: "replyAll", forward: "forward"})[root.mode] || "reply"; size: 14; color: Theme.amber }
                }
                Text {
                    objectName: "composerTitleText"
                    Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.PlainText
                    text: ({new: "Compose Email", reply: "Reply", reply_all: "Reply All", forward: "Forward"})[root.mode] || "Reply"
                    color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(19); font.weight: Font.Medium
                    font.letterSpacing: 0; lineHeight: 1.1
                }
            }
            Text {
                objectName: "archivedDraftNotice"; visible: backend.accountArchived || backend.unsavedAccountBuffer
                Layout.fillWidth: true; Layout.bottomMargin: 12; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: backend.unsavedAccountBuffer ? backend.draftStatus : "Saved account archive · selectable preview. Editing, sending and sync are off."
                color: backend.unsavedAccountBuffer ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
            RowLayout {
                visible: !root.readOnly
                Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
                Text { text: "From"; Layout.preferredWidth: 44; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                Button {
                    id: fromButton; objectName: "draftFromAccount"
                    Layout.fillWidth: true; Layout.maximumWidth: contentItem.implicitWidth + 20
                    implicitHeight: 30; leftPadding: 0; rightPadding: 4
                    enabled: backend.draftTransfer.can_change && backend.accounts.length > 1
                    Accessible.name: "From account, " + (backend.draft.from || "")
                    onClicked: fromMenu.open()
                    background: Rectangle { x: -6; width: fromButton.width + 8; color: fromButton.hovered || fromButton.down ? Theme.hover : "transparent"; radius: 5; border.color: fromButton.visualFocus ? Theme.accent : "transparent" }
                    contentItem: RowLayout {
                        spacing: 6
                        Text { Layout.fillWidth: true; text: (backend.draft.from_name || backend.currentAccount.name || "")+" · "+(backend.draft.from || ""); color: fromButton.enabled ? Theme.text : Theme.subtle; textFormat: Text.PlainText; elide: Text.ElideRight; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                        FmIcon { name: "chevronDown"; size: 10; color: Theme.inkAlpha(.4); visible: backend.accounts.length > 1 }
                    }
                    Menu {
                        id: fromMenu; objectName: "draftFromMenu"; y: fromButton.height
                        padding: 5
                        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
                        width: Math.min(420, root.width - 30)
                        Instantiator {
                            model: backend.accounts
                            delegate: MenuItem {
                                id: fromOption
                                required property var modelData
                                implicitHeight: 36; leftPadding: 8; rightPadding: 8
                                text: (modelData.name || modelData.email || "") + (modelData.email ? " · " + modelData.email : "")
                                checkable: true; checked: modelData.id === backend.account
                                indicator: null
                                background: Rectangle { color: fromOption.highlighted || fromOption.hovered ? Theme.hover : "transparent"; radius: 5 }
                                contentItem: RowLayout {
                                    spacing: 8
                                    FmIcon { name: "check"; size: 14; color: Theme.text; opacity: fromOption.checked ? 1 : 0 }
                                    Text { Layout.fillWidth: true; text: fromOption.text; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                                }
                                onTriggered: backend.changeDraftAccount(modelData.id)
                            }
                            onObjectAdded: (index, object) => fromMenu.insertItem(index, object)
                            onObjectRemoved: (index, object) => fromMenu.removeItem(object)
                        }
                    }
                }
                Item { Layout.fillWidth: true }
            }
            Item { visible: !root.readOnly; Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.bg6 } }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 6
                visible: (backend.draftTransfer.status || "").length > 0
                Text { objectName: "draftTransferStatus"; Layout.fillWidth: true; text: backend.draftTransfer.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                Flow {
                    Layout.fillWidth: true; spacing: 8
                    FmButton { objectName: "retryDraftTransfer"; text: "Check account change"; visible: backend.draftTransfer.retry; enabled: !backend.draftTransfer.busy; onClicked: backend.retryDraftTransfer() }
                    FmButton { objectName: "retryDraftTransferCleanup"; text: "Retry account change"; visible: backend.draftTransfer.cleanup_retry; enabled: !backend.draftTransfer.busy; onClicked: backend.retryDraftTransferCleanup() }
                    FmButton { objectName: "openDraftTransferDestination"; text: "Open moved draft"; visible: backend.draftTransfer.transferred; onClicked: backend.openDraftTransferDestination() }
                }
            }
            RecipientField { id: toField; visible: !root.readOnly; Layout.fillWidth: true; label: "To"; fieldName: "to_recipients"; showLinks: true; onCcRequested: root.ccOpen=true; onBccRequested: root.bccOpen=true }
            RecipientField { id: ccField; Layout.fillWidth: true; label: "Cc"; fieldName: "cc_recipients"; visible: !root.readOnly && (root.ccOpen || (backend.draft.cc_recipients||[]).length>0 || (backend.recipientInputs.cc_recipients||"").length>0) }
            RecipientField { id: bccField; Layout.fillWidth: true; label: "Bcc"; fieldName: "bcc_recipients"; visible: !root.readOnly && (root.bccOpen || (backend.draft.bcc_recipients||[]).length>0 || (backend.recipientInputs.bcc_recipients||"").length>0) }
            RowLayout {
                Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
                Text { text: "Subject"; Layout.preferredWidth: 44; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                TextField { objectName: "draftSubject"; property bool blocksWorkspaceShortcuts: true; Layout.fillWidth: true; padding: 0; leftPadding: 0; rightPadding: 0; text: backend.draft.subject || ""; placeholderText: "Subject"; color: Theme.text; placeholderTextColor: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13); font.weight: Font.Medium; background: Item {} readOnly: root.readOnly || backend.draftLocked; selectByMouse: true; onTextEdited: backend.setDraftSubject(text) }
            }
            Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.bg6 } }
            ScrollView {
                id: composerScroll
                objectName: "replyBodyScroll"
                onHeightChanged: Qt.callLater(root.measureChrome)
                ScrollBar.vertical: FmScrollBar { policy: bodyEditor.implicitHeight > composerScroll.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
                Layout.fillWidth: true; Layout.fillHeight: true
                // The body grows with its text; past the cap it scrolls inside.
                Layout.minimumHeight: root.mode === "new" ? 96 : 140
                Layout.preferredHeight: Math.max(Layout.minimumHeight, Math.min(Math.max(400, (root.minimumHeight || 560) - 200),
                    root.availableHeight > 0 ? root.availableHeight - root.chromeHeight : Infinity, bodyEditor.implicitHeight))
                contentWidth: availableWidth
                clip: true
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                TextArea {
                    id: bodyEditor
                    objectName: "replyComposer"
                    property bool blocksWorkspaceShortcuts: true
                    width: composerScroll.availableWidth
                    // A lock while Gmail or Send works keeps the cursor here:
                    // the editor refuses edits instead of dropping focus.
                    readOnly: root.readOnly || backend.draftLocked
                    textFormat: TextEdit.RichText
                    selectByMouse: true
                    persistentSelection: true
                    wrapMode: TextEdit.Wrap
                    placeholderText: root.readOnly ? "" : "Write it yourself — nothing here drafts for you."
                    placeholderTextColor: Theme.subtle
                    color: Theme.text
                    selectionColor: Theme.selection
                    selectedTextColor: Theme.selectedText
                    font { family: Theme.fontFamily; pointSize: Theme.fontPoints(14) }
                    background: null
                    padding: 0; leftPadding: 0; rightPadding: 0; topPadding: 16; bottomPadding: forwarded.visible ? forwarded.height + 28 : 8
                    Accessible.name: root.draftTitle+" body"
                    // ⌘B, ⌘I and ⌘U style the selection or what is typed next;
                    // ⌘⇧8 and ⌘⇧7 make bulleted and numbered lists.
                    Keys.onPressed: event => {
                        if (!(event.modifiers & Qt.ControlModifier) || root.readOnly) return
                        const shift = event.modifiers & Qt.ShiftModifier
                        const kind = event.key === Qt.Key_B ? "bold" : event.key === Qt.Key_I ? "italic" : event.key === Qt.Key_U ? "underline"
                            : shift && (event.key === Qt.Key_8 || event.key === Qt.Key_Asterisk) ? "bullet"
                            : shift && (event.key === Qt.Key_7 || event.key === Qt.Key_Ampersand) ? "numbered" : ""
                        if (kind) { document.format(kind); event.accepted = true }
                    }
                    ContextMenu.menu: null
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: function(point) {
                            contextMenu.spelling = bodyEditor.readOnly ? ({}) : document.spellingAt(bodyEditor.positionAt(point.position.x, point.position.y))
                            contextMenu.show(bodyEditor, point.position.x, point.position.y)
                        }
                    }
                    // The system spell checker's misspelled words: a dotted red
                    // rule under each. Drawn over the text, never saved with it.
                    Repeater {
                        model: root.readOnly ? [] : document.misspellings
                        Shape {
                            id: misspelling
                            required property var modelData
                            // Relayout (typing, wrapping, text size) moves the words. The
                            // marks catch up a moment after the text changes; one that
                            // reaches past the current text draws nothing meanwhile.
                            readonly property bool current: { bodyEditor.contentHeight; bodyEditor.width; return modelData.start + modelData.length <= bodyEditor.length }
                            readonly property rect first: { bodyEditor.contentHeight; bodyEditor.width; return modelData.start + modelData.length <= bodyEditor.length ? bodyEditor.positionToRectangle(modelData.start) : Qt.rect(0, 0, 0, 0) }
                            readonly property rect last: { bodyEditor.contentHeight; bodyEditor.width; return modelData.start + modelData.length <= bodyEditor.length ? bodyEditor.positionToRectangle(modelData.start + modelData.length) : Qt.rect(0, 0, 0, 0) }
                            visible: current
                            x: first.x; y: first.y + first.height - 2
                            width: Math.max(4, last.y === first.y ? last.x - first.x : bodyEditor.width - bodyEditor.rightPadding - first.x); height: 3
                            preferredRendererType: Shape.CurveRenderer
                            ShapePath {
                                strokeColor: Theme.error; strokeWidth: 1.6; fillColor: "transparent"
                                strokeStyle: ShapePath.DashLine; dashPattern: [0.9, 1.4]; capStyle: ShapePath.RoundCap
                                startX: 1; startY: 1.5
                                PathLine { x: misspelling.width - 1; y: 1.5 }
                            }
                        }
                    }
                    // An HTML forward's original sits in the editor's bottom
                    // padding, under the author's words, so the message area
                    // scrolls through both. It is shown, not edited.
                    Column {
                        id: forwarded
                        objectName: "forwardedOriginal"
                        visible: root.forwardsOriginal
                        y: bodyEditor.topPadding + bodyEditor.contentHeight + 20; width: bodyEditor.width
                        spacing: 12
                        Text {
                            objectName: "forwardedOriginalHeader"
                            width: parent.width; wrapMode: Text.Wrap; textFormat: Text.PlainText
                            text: "---------- Forwarded message ----------\n" + (root.forwardedMessage
                                ? "From: " + (root.forwardedMessage.sender_email || "") + "\nDate: " + (root.forwardedMessage.sent_at || "")
                                    + "\nSubject: " + (Theme.oneLine(root.forwardedMessage.subject) || "(no subject)")
                                : "It is sent as received. Open the conversation it came from to see it here.")
                            color: Theme.inkAlpha(.7); font: bodyEditor.font
                        }
                        Loader {
                            objectName: "forwardedOriginalView"
                            width: parent.width
                            active: forwarded.visible && !!root.forwardedMessage && root.forwardedView !== null
                            sourceComponent: root.forwardedView
                        }
                    }
                    // Pasting keeps the draft's own formatting: text comes in plain.
                    // A misspelled word under the pointer puts the system's
                    // suggestions first.
                    TextMenu {
                        id: contextMenu
                        property var spelling: ({})
                        readonly property var guesses: spelling.suggestions || []
                        function respell(word) { close(); document.replaceWord(spelling.start, spelling.word, word); bodyEditor.forceActiveFocus() }
                        leading: [
                            Repeater {
                                model: contextMenu.guesses
                                FmMenuItem {
                                    required property string modelData
                                    required property int index
                                    objectName: "spellingSuggestion_" + index
                                    text: modelData; font.weight: Font.Medium
                                    onClicked: contextMenu.respell(modelData)
                                }
                            },
                            FmMenuItem { objectName: "spellingNoGuesses"; visible: !!contextMenu.spelling.word && contextMenu.guesses.length === 0; text: "No guesses found"; enabled: false },
                            FmMenuItem { objectName: "spellingIgnore"; visible: !!contextMenu.spelling.word; separatorAbove: true; text: "Ignore spelling"; onClicked: { contextMenu.close(); document.ignoreSpelling(contextMenu.spelling.word) } },
                            FmMenuItem { objectName: "spellingLearn"; visible: !!contextMenu.spelling.word; text: "Learn spelling"; onClicked: { contextMenu.close(); document.learnSpelling(contextMenu.spelling.word) } }
                        ]
                        pasteLabel: document.clipboardAttachable ? "Paste as attachment" : "Paste plain text"; pasteText: function() { document.pastePlain() }
                        pasteAvailable: bodyEditor.canPaste || document.clipboardAttachable
                        FmMenuItem { separatorAbove: true; text: "Bold"; shortcut: contextMenu.key + "B"; enabled: !root.readOnly; onClicked: { contextMenu.close(); document.format("bold") } }
                        FmMenuItem { text: "Italic"; shortcut: contextMenu.key + "I"; enabled: !root.readOnly; onClicked: { contextMenu.close(); document.format("italic") } }
                        FmMenuItem { text: "Underline"; shortcut: contextMenu.key + "U"; enabled: !root.readOnly; onClicked: { contextMenu.close(); document.format("underline") } }
                        FmMenuItem { text: "Bulleted list"; shortcut: contextMenu.key + "⇧8"; enabled: !root.readOnly; onClicked: { contextMenu.close(); document.format("bullet") } }
                        FmMenuItem { text: "Numbered list"; shortcut: contextMenu.key + "⇧7"; enabled: !root.readOnly; onClicked: { contextMenu.close(); document.format("numbered") } }
                    }
                }
            }
            Flow {
                id: attachmentChips
                visible: (backend.draft.attachments||[]).length>0
                Layout.fillWidth: true
                Layout.bottomMargin: 12
                spacing: 6
                Repeater {
                    model: backend.draft.attachments || []
                    Rectangle {
                        id: attachmentChip
                        required property var modelData
                        width: Math.min(320, attachmentChips.width, chipName.implicitWidth + chipSize.implicitWidth + (root.readOnly ? 104 : 72))
                        height: 30; radius: 5; color: "transparent"; border.color: Theme.controlBorder
                        RowLayout {
                            anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 6; spacing: 8
                            FmIcon { name: "attachment"; size: 14; color: Theme.secondary }
                            Text { id: chipName; Layout.fillWidth: true; text: attachmentChip.modelData.name; elide: Text.ElideRight; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                            Text { id: chipSize; text: root.attachmentSize(attachmentChip.modelData.size); color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                            FmButton {
                                objectName: "exportDraftAttachment_" + attachmentChip.modelData.id
                                visible: root.readOnly; text: "Open"; implicitHeight: 24; leftPadding: 2; rightPadding: 2
                                enabled: attachmentChip.modelData.state === "available"
                                onClicked: backend.openAttachment(attachmentChip.modelData.id)
                            }
                            FmButton {
                                objectName: "removeDraftAttachment_" + attachmentChip.modelData.id
                                visible: !root.readOnly
                                implicitWidth: 18; implicitHeight: 18; leftPadding: 0; rightPadding: 0
                                iconName: "close"; iconSize: 9
                                tip: "Remove attachment " + attachmentChip.modelData.name
                                enabled: !backend.draftLocked
                                onClicked: backend.setDraftAttachment(attachmentChip.modelData.id, false)
                            }
                        }
                    }
                }
            }
            // Always there while the draft can be edited, so nothing moves as text
            // is selected. A style lights up where the caret or selection has it.
            RowLayout {
                objectName: "formatBar"
                visible: !root.readOnly
                Layout.fillWidth: true; Layout.bottomMargin: 4
                spacing: 1
                readonly property string key: Qt.platform.os === "osx" ? "⌘" : "Ctrl+"
                readonly property var formats: document.formats
                FmButton { objectName: "formatBold"; text: "B"; font.bold: true; selected: !!parent.formats.bold; tip: "Bold · " + parent.key + "B"; enabled: !backend.draftLocked; onClicked: document.format("bold") }
                FmButton { objectName: "formatItalic"; text: "I"; font.italic: true; selected: !!parent.formats.italic; tip: "Italic · " + parent.key + "I"; enabled: !backend.draftLocked; onClicked: document.format("italic") }
                FmButton { objectName: "formatUnderline"; text: "U"; font.underline: true; selected: !!parent.formats.underline; tip: "Underline · " + parent.key + "U"; enabled: !backend.draftLocked; onClicked: document.format("underline") }
                FmButton { objectName: "formatBullets"; iconName: "bulletList"; selected: !!parent.formats.bullet; tip: "Bulleted list · " + parent.key + "⇧8"; enabled: !backend.draftLocked; Layout.leftMargin: 8; onClicked: document.format("bullet") }
                FmButton { objectName: "formatNumbers"; iconName: "numberedList"; selected: !!parent.formats.numbered; tip: "Numbered list · " + parent.key + "⇧7"; enabled: !backend.draftLocked; onClicked: document.format("numbered") }
                FmButton { iconName: "undo"; tip: "Undo"; enabled: bodyEditor.canUndo && !backend.draftBusy; Layout.leftMargin: 8; onClicked: document.undo() }
                FmButton { iconName: "redo"; tip: "Redo"; enabled: bodyEditor.canRedo && !backend.draftBusy; onClicked: document.redo() }
                Item { Layout.fillWidth: true }
            }
            // Problems that need the person; everything else stays in the one hint line.
            RowLayout {
                visible: !root.readOnly && (backend.attachmentImport.status || "").length > 0
                Layout.fillWidth: true; Layout.bottomMargin: 8; spacing: 8
                Text { objectName: "draftAttachmentStatus"; Layout.fillWidth: true; text: backend.attachmentImport.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.attachmentImport.retry ? Theme.warn : text.indexOf("Not attached") === 0 ? Theme.error : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                FmButton { objectName: "retryDraftAttachment"; visible: backend.attachmentImport.retry || backend.attachmentImport.can_resume || false; text: backend.attachmentImport.retry ? "Retry attachment" : "Continue"; outlined: true; enabled: backend.connected && !backend.attachmentImportBusy && !backend.draftBusy; onClicked: { backend.clearError(); backend.retryDraftAttachment() } }
                FmButton { objectName: "skipQueuedAttachments"; visible: (backend.attachmentImport.queued || 0) > 0; text: "Skip remaining"; onClicked: backend.skipQueuedDraftAttachments() }
            }
            ColumnLayout {
                visible: backend.googleSend.visible || false
                Layout.fillWidth: true; Layout.bottomMargin: visible ? 8 : 0; spacing: 6
                Text { objectName: "googleSendStatus"; Layout.fillWidth: true; text: backend.googleSend.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.googleSend.warning ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    FmButton { objectName: "resumeSyncToSend"; text: "Resume sync"; outlined: true; visible: backend.googleSend.can_resume_sync || false; onClicked: backend.startGoogleSync(backend.account, {}) }
                    FmButton { objectName: "checkGoogleSend"; text: backend.googleSend.check_label || "Check status"; outlined: true; visible: backend.googleSend.can_check || false; onClicked: backend.checkGoogleSend() }
                    FmButton { objectName: "showCurrentGoogleSend"; text: "View current send"; visible: backend.googleSend.can_select_current || false; onClicked: backend.showCurrentGoogleSend() }
                    FmButton { objectName: "resumeGoogleSend"; text: backend.googleSend.resume_label || "Send prepared version"; visible: backend.googleSend.can_resume || false; onClicked: backend.resumeGoogleSend() }
                    FmButton { objectName: "abandonGoogleSend"; text: "Abandon prepared send"; visible: backend.googleSend.can_abandon || false; onClicked: backend.abandonGoogleSend() }
                    FmButton { objectName: "cancelGoogleSend"; text: "Cancel"; visible: backend.googleSend.can_cancel || false; onClicked: backend.cancelGoogleSend() }
                }
            }
            ColumnLayout {
                visible: backend.draftDiscard.visible || false
                Layout.fillWidth: true; Layout.bottomMargin: visible ? 8 : 0; spacing: 6
                Text { Layout.fillWidth: true; text: backend.draftDiscard.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.warn; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                RowLayout {
                    spacing: 6
                    FmButton { objectName: "checkDraftDiscard"; text: "Check discard"; visible: backend.draftDiscard.can_check || false; onClicked: backend.checkDraftDiscard() }
                    FmButton { objectName: "retryDraftDiscard"; text: backend.draftDiscard.retry_label || "Retry discard"; visible: backend.draftDiscard.can_retry || false; onClicked: backend.retryDraftDiscard() }
                }
            }
            Text {
                objectName: "draftPgpMissing"
                // With Encrypt on, a recipient without a key stops the send; with it
                // off where the account encrypts by default, this says why.
                visible: root.pgpShown && root.pgpKnown && root.pgpMissing.length > 0 && (root.pgpEncrypt || root.pgpEncryptDefault)
                Layout.fillWidth: true; Layout.bottomMargin: 8; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: root.pgpEncrypt
                    ? "No OpenPGP key for " + root.pgpMissing.join(", ") + ". Import " + (root.pgpMissing.length === 1 ? "their key" : "their keys") + " in Settings → Encryption, or turn off Encrypt to send it unencrypted."
                    : "Not encrypted: no OpenPGP key for " + root.pgpMissing.join(", ") + ". Import " + (root.pgpMissing.length === 1 ? "their key" : "their keys") + " in Settings → Encryption to encrypt."
                color: root.pgpEncrypt ? Theme.warn : Theme.inkAlpha(.5); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
            }
            Text {
                objectName: "draftPgpReplyClear"
                // A reply to encrypted mail starts encrypted; turned off, its
                // text (and the quote of what was encrypted) goes out readable.
                visible: root.pgpShown && backend.draft.replies_to_encrypted === true && !root.pgpEncrypt
                Layout.fillWidth: true; Layout.bottomMargin: 8; wrapMode: Text.Wrap; textFormat: Text.PlainText
                text: "You're replying to encrypted mail without encryption. This reply, and any quote of the message, is saved to Gmail Drafts and sent readable."
                color: Theme.warn; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5)
            }
            RowLayout {
                visible: backend.draftStatus.indexOf("Not saved") >= 0 && !backend.draftConflict
                Layout.bottomMargin: 8
                FmButton { text: "Retry save"; outlined: true; enabled: !backend.draftLocked; onClicked: { backend.clearError(); backend.saveDraft() } }
                FmButton { text: "Reload saved draft…"; enabled: !backend.draftLocked; onClicked: reloadDialog.open() }
            }
            ColumnLayout {
                visible: backend.draftConflict
                Layout.fillWidth: true; Layout.bottomMargin: 8; spacing: 8
                Text { Layout.fillWidth: true; text: "Another window saved a newer version of this draft. Your text, formatting and attachments are still here. Choose which version to keep."; color: Theme.error; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11.5) }
                RowLayout {
                    FmButton { objectName: "saveMyDraftVersion"; text: "Save my version…"; outlined: true; enabled: !backend.draftBusy; onClicked: conflictDialog.open() }
                    FmButton { text: "Reload saved draft…"; enabled: !backend.draftBusy; onClicked: reloadDialog.open() }
                }
            }
            Text {
                Layout.fillWidth: true; Layout.bottomMargin: 8
                visible: backend.captureStatus.length > 0
                text: backend.captureStatus
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: Theme.muted
                font { family: Theme.fontFamily; pointSize: Theme.fontPoints(10.5) }
            }
            Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.bg6 } }
            RowLayout {
                objectName: "composerFooter"
                Layout.fillWidth: true; Layout.topMargin: 12; Layout.bottomMargin: 12; spacing: 6
                FmButton {
                    objectName: "captureReply"
                    visible: !root.readOnly
                    text: backend.syntheticProfile ? "Capture locally" : backend.googleSend.busy ? "Sending…" : "Send"
                    prominent: true
                    implicitHeight: 32; leftPadding: 16; rightPadding: 16
                    font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium
                    enabled: backend.syntheticProfile ? !backend.draftLocked && !backend.attachmentImport.pending && (document.plainText.trim().length > 0 || root.attachmentIds.length > 0) : backend.googleSend.can_send || false
                    tip: backend.syntheticProfile ? "Record an outgoing MIME copy on this machine; no email is delivered" : "Send through Gmail"
                    onClicked: { if (backend.syntheticProfile) backend.captureDraft(); else backend.sendGoogleDraft() }
                }
                FmButton { objectName: "draftAttachButton"; visible: !root.readOnly; text: "Attach"; iconName: "attachment"; outlined: true; implicitHeight: 32; leftPadding: 12; rightPadding: 12; font.weight: Font.Medium; font.pointSize: Theme.fontPoints(12); tip: "Attach files — or drop them anywhere on the message"; enabled: backend.connected && !backend.draftBusy && !backend.attachmentImportBusy; onClicked: root.chooseLocalAttachment() }
                FmButton {
                    objectName: "draftPgpSign"; visible: root.pgpShown
                    text: "Sign"; iconName: "shield"; iconSize: 13; outlined: !root.pgpSign; selected: root.pgpSign
                    implicitHeight: 32; leftPadding: 12; rightPadding: 12; font.weight: Font.Medium; font.pointSize: Theme.fontPoints(12)
                    tip: root.pgpSign ? "Signed with your OpenPGP key, so recipients can check it's from you · click to stop" : "Sign with your OpenPGP key"
                    enabled: !backend.draftLocked && !backend.draftBusy
                    onClicked: backend.setDraftProtection(!root.pgpSign, root.pgpEncrypt)
                    Accessible.role: Accessible.CheckBox; Accessible.checked: root.pgpSign
                }
                // Encrypt turns on only once every recipient has a key; while it
                // can't, hovering says why.
                Item {
                    visible: root.pgpShown
                    implicitWidth: encryptButton.implicitWidth; implicitHeight: encryptButton.implicitHeight
                    FmButton {
                        id: encryptButton
                        objectName: "draftPgpEncrypt"; anchors.fill: parent
                        text: "Encrypt"; iconName: "lock"; iconSize: 13; outlined: !root.pgpEncrypt; selected: root.pgpEncrypt
                        implicitHeight: 32; leftPadding: 12; rightPadding: 12; font.weight: Font.Medium; font.pointSize: Theme.fontPoints(12)
                        tip: root.pgpEncrypt ? "Only the recipients and you can read it; the subject stays visible · this draft isn't saved to Gmail Drafts" : "Encrypt to every recipient's OpenPGP key"
                        enabled: !backend.draftLocked && !backend.draftBusy && (root.pgpEncrypt || root.pgpReady)
                        onClicked: root.choosePgpEncrypt(!root.pgpEncrypt)
                        Accessible.role: Accessible.CheckBox; Accessible.checked: root.pgpEncrypt
                    }
                    HoverHandler { id: encryptHover }
                    FmToolTip {
                        shown: encryptHover.hovered && !encryptButton.enabled && !backend.draftLocked && !backend.draftBusy
                        text: !root.pgpRecipients.length ? "Add recipients to encrypt" : !root.pgpKnown ? "Checking recipients' keys…"
                            : "No OpenPGP key for " + root.pgpMissing.join(", ") + " · import theirs in Settings → Encryption"
                    }
                }
                Text {
                    objectName: "composerHint"
                    Layout.fillWidth: true
                    visible: !root.readOnly
                    text: root.hint.text
                    horizontalAlignment: Text.AlignRight
                    color: root.hint.tone
                    elide: Text.ElideRight; textFormat: Text.PlainText
                    font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11.5) }
                    HoverHandler { id: hintHover }
                    FmToolTip { shown: hintHover.hovered && parent.truncated; text: parent.text }
                }
                FmButton {
                    id: discardButton
                    objectName: "discardReply"
                    visible: !root.readOnly
                    text: "Discard"; iconName: "bin"; iconSize: 13
                    implicitHeight: 32; leftPadding: 12; rightPadding: 12; Layout.leftMargin: 10
                    font.pointSize: Theme.fontPoints(12); font.weight: Font.Medium
                    foreground: !enabled ? Theme.subtle : hovered ? Theme.error : Theme.inkAlpha(.8)
                    tip: "Discard this message"
                    enabled: !backend.draftLocked && !backend.draftBusy && !backend.attachmentImport.pending
                    background: Rectangle { radius: 5; color: discardButton.hovered ? Theme.errorAlpha(.08) : "transparent"; border.width: 1; border.color: discardButton.visualFocus ? Theme.focusBorder : discardButton.hovered ? Theme.errorAlpha(.4) : Theme.line2 }
                    onClicked: root.confirmDiscard()
                }
            }
        }
    }
    ColumnLayout {
        id: recent
        objectName: "recentWith"
        visible: root.recentVisible
        y: composer.height + 28; x: 20; width: parent.width - 20
        spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 28; spacing: 12
            Text { text: "Recent with " + root.recentName; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11) }
            Rectangle { Layout.fillWidth: true; implicitHeight: Theme.ruleWidth; color: Theme.line }
            FmButton {
                objectName: "recentWithAll"
                text: "All mail with " + root.recentName + " (" + (backend.recentWith.total || 0) + ")"
                implicitHeight: 20; topPadding: 0; bottomPadding: 0; leftPadding: 0; rightPadding: 0
                font.pointSize: Theme.fontPoints(11); foreground: hovered ? Theme.text : Theme.inkAlpha(.7); background: Item {}
                onClicked: root.allMailWith(backend.recentWith.email || "")
            }
        }
        Repeater {
            model: root.recentVisible ? backend.recentWith.rows : []
            ItemDelegate {
                id: recentRow
                required property var modelData
                readonly property bool outgoing: String(modelData.sender_email || "").toLowerCase() === String(backend.currentAccount.email || "").toLowerCase()
                objectName: "recentWith_" + modelData.id
                Layout.fillWidth: true; implicitHeight: 32
                leftPadding: 0; rightPadding: 0; topPadding: 0; bottomPadding: 0; hoverEnabled: true
                background: Rectangle {
                    color: recentRow.hovered ? Theme.inkAlpha(.025) : "transparent"
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: Theme.ruleWidth; color: Theme.inkAlpha(.05) }
                }
                FmToolTip { shown: recentRow.hovered; text: (recentRow.outgoing ? "You wrote" : "From " + (recentRow.modelData.sender_name || recentRow.modelData.sender_email || "")) + " · opens the thread (this message is kept in Drafts)" }
                onClicked: root.recentOpened(modelData.id)
                contentItem: RowLayout {
                    spacing: 10
                    FmIcon { name: "arrowRight"; size: 11; color: Theme.inkAlpha(.4); rotation: recentRow.outgoing ? 0 : 180; Layout.preferredWidth: 14 }
                    Text {
                        Layout.fillWidth: true; elide: Text.ElideRight; textFormat: Text.StyledText
                        text: "<font color=\"" + Theme.inkAlpha(.85) + "\">" + (Theme.oneLine(recentRow.modelData.subject) || "(No subject)").replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</font><font color=\"" + Theme.inkAlpha(.45) + "\"> · " + String(recentRow.modelData.snippet || "").replace(/\s+/g, " ").replace(/&/g, "&amp;").replace(/</g, "&lt;") + "</font>"
                        font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
                    }
                    Text { text: root.dayLabel(recentRow.modelData.latest_at); color: Theme.inkAlpha(.4); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(11); font.features: {"tnum": 1} }
                }
            }
        }
    }
    FileDialog {
        id: localAttachmentDialog
        objectName: "draftAttachmentFileDialog"
        property string targetAccount: ""
        property string targetDraft: ""
        title: "Attach files"
        fileMode: FileDialog.OpenFiles
        onAccepted: backend.importDraftAttachments(selectedFiles, targetAccount, targetDraft)
    }
    FmConfirmDialog {
        id: discardDialog; objectName: "discardDraftDialog"
        property var capturedContext: ({})
        title: "Discard this draft?"
        message: "It will be removed from Drafts here and in Gmail. You can undo for a few seconds afterwards."
        actionText: "Discard"; destructive: true
        actionObjectName: "confirmDiscardDraft"
        onAccepted: backend.discardDraft(capturedContext)
    }
    FmConfirmDialog {
        id: reloadDialog
        title: "Reload the saved draft?"
        message: "This replaces your unsaved edits with the last saved version."
        actionText: "Reload"; destructive: true
        onAccepted: { backend.clearError(); backend.reloadDraft() }
    }
    FmConfirmDialog {
        id: conflictDialog; objectName: "draftConflictDialog"
        title: "Save your version?"
        message: "Replace the saved draft with your text, formatting and attachments. If it changes again while saving, you can review the new conflict."
        actionText: "Save my version"; actionObjectName: "confirmSaveMyDraft"
        onAccepted: backend.saveMyDraftVersion()
    }
}
