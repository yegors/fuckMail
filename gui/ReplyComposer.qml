import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

Rectangle {
    id: root
    signal dismissed()
    property bool loading: false
    property bool signatureOpen: false
    property bool ccOpen: false
    property bool bccOpen: false
    readonly property bool popupOpen: discardDialog.opened || reloadDialog.opened || conflictDialog.opened || googleDraftDialog.opened || attachmentMenu.opened || contextMenu.opened || localAttachmentDialog.visible
    readonly property string mode: backend.draft.mode || "reply"
    readonly property string draftTitle: ({new:"New message",reply:"Reply",reply_all:"Reply all",forward:"Forward"})[mode] || "Reply"
    readonly property bool hasSelection: bodyEditor.selectionEnd > bodyEditor.selectionStart
    readonly property var attachmentIds: backend.draft.attachment_ids || []
    readonly property var availableAttachments: {
        let items = []
        for (const message of (backend.conversation.messages || []))
            for (const item of (message.attachments || [])) if (item.state === "available") items.push(item)
        return items
    }
    function focusEditor() { bodyEditor.forceActiveFocus() }
    function chooseLocalAttachment() {
        localAttachmentDialog.targetAccount = backend.account
        localAttachmentDialog.targetDraft = backend.draft.draft_id || ""
        localAttachmentDialog.open()
    }
    function attachmentSize(bytes) {
        return bytes >= 1048576 ? (bytes / 1048576).toFixed(1) + " MB" : bytes >= 1024 ? Math.ceil(bytes / 1024) + " KB" : bytes + " B"
    }
    function confirmDiscard() { if (!backend.draftLocked && !backend.attachmentImport.pending) discardDialog.open() }
    function syncDraft() {
        loading = true
        document.setIdentity(backend.account + "/" + (backend.activeDraftId || backend.selectedId))
        document.load(backend.draft.body || "", backend.draft.body_html || "")
        if (signature.text !== (backend.draft.signature_text || "")) signature.text = backend.draft.signature_text || ""
        loading = false
    }
    color: "transparent"
    implicitHeight: contents.implicitHeight
    Rectangle { width: 2; height: parent.height; color: Theme.accentAlpha(0.55) }
    Component.onCompleted: syncDraft()
    Connections {
        target: backend
        function onDraftChanged() {
            root.syncDraft()
            if (backend.draftStatus === "Draft discarded") root.dismissed()
        }
    }
    ReplyDocument {
        id: document
        objectName: "replyDocument"
        textDocument: bodyEditor.textDocument
        editor: bodyEditor
        onContentChanged: if (!root.loading) backend.editFormattedDraft(plainText, html)
    }
    ColumnLayout {
        id: contents
        anchors { top: parent.top; left: parent.left; right: parent.right; leftMargin: 20 }
        spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.bottomMargin: 14; spacing: 10
            Text { text: root.draftTitle; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(15); weight: Font.Medium; letterSpacing: -0.15 } }
            Text { visible: root.mode!=="new"; text: root.mode==="forward" ? "original message quoted below" : "in this conversation"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); Layout.fillWidth: true; elide: Text.ElideRight }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "collapseReply"; text: "Minimise"; font.pixelSize: Theme.fontSize(12); leftPadding: 0; rightPadding: 0; tip: "Keep this draft and hide the editor"; onClicked: { backend.saveDraft(); root.dismissed() } }
            FmButton { objectName: "discardReply"; text: "Discard"; font.pixelSize: Theme.fontSize(12); leftPadding: 4; rightPadding: 0; enabled: !backend.draftLocked && !backend.attachmentImport.pending; onClicked: discardDialog.open() }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "From"; Layout.preferredWidth: 44; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            Text { text: (backend.draft.from_name || backend.currentAccount.name || "")+" · "+(backend.draft.from || ""); color: Theme.text; textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        RecipientField { Layout.fillWidth: true; label: "To"; fieldName: "to_recipients"; showLinks: true; onCcRequested: root.ccOpen=true; onBccRequested: root.bccOpen=true }
        RecipientField { Layout.fillWidth: true; label: "Cc"; fieldName: "cc_recipients"; visible: root.ccOpen || (backend.draft.cc_recipients||[]).length>0 }
        RecipientField { Layout.fillWidth: true; label: "Bcc"; fieldName: "bcc_recipients"; visible: root.bccOpen || (backend.draft.bcc_recipients||[]).length>0 }
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "Subject"; Layout.preferredWidth: 44; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            TextField { objectName: "draftSubject"; property bool blocksWorkspaceShortcuts: true; Layout.fillWidth: true; padding: 0; text: backend.draft.subject || ""; placeholderText: "Subject"; color: Theme.text; placeholderTextColor: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); font.weight: Font.Medium; background: Item {} enabled: !backend.draftLocked; selectByMouse: true; onTextEdited: backend.setDraftSubject(text) }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        ScrollView {
            id: composerScroll
            objectName: "replyBodyScroll"
            Layout.fillWidth: true
            Layout.topMargin: 16; Layout.bottomMargin: 8
            Layout.preferredHeight: Math.max(root.mode==="new" ? 230 : 150, Math.min(400, bodyEditor.contentHeight + 16))
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        TextArea {
            id: bodyEditor
            objectName: "replyComposer"
            property bool blocksWorkspaceShortcuts: true
            width: composerScroll.availableWidth
            enabled: !backend.draftLocked
            textFormat: TextEdit.RichText
            selectByMouse: true
            persistentSelection: true
            wrapMode: TextEdit.Wrap
            placeholderText: "Write it yourself — nothing here drafts for you."
            placeholderTextColor: Theme.subtle
            color: Theme.text
            selectionColor: Theme.selection
            selectedTextColor: Theme.selectedText
            font { family: Theme.fontFamily; pixelSize: Theme.fontSize(15) }
            background: null
            padding: 0
            Accessible.name: root.draftTitle+" body"
            Keys.onPressed: event => {
                if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_B && root.hasSelection) { document.format("bold"); event.accepted = true }
                if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_I && root.hasSelection) { document.format("italic"); event.accepted = true }
                if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_U && root.hasSelection) { document.format("underline"); event.accepted = true }
            }
            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: contextMenu.popup()
            }
            Menu {
                id: contextMenu
                MenuItem { text: "Copy"; enabled: root.hasSelection; onTriggered: bodyEditor.copy() }
                MenuItem { text: "Cut"; enabled: root.hasSelection; onTriggered: bodyEditor.cut() }
                MenuItem { text: "Paste plain text"; onTriggered: document.pastePlain() }
                MenuSeparator {}
                MenuItem { text: "Bold"; enabled: root.hasSelection; onTriggered: document.format("bold") }
                MenuItem { text: "Italic"; enabled: root.hasSelection; onTriggered: document.format("italic") }
                MenuItem { text: "Underline"; enabled: root.hasSelection; onTriggered: document.format("underline") }
                MenuItem { text: "Edit signature"; onTriggered: root.signatureOpen = true }
                MenuSeparator {}
                MenuItem { text: "Select all"; onTriggered: bodyEditor.selectAll() }
            }
        }
        }
        Text {
            visible: !root.signatureOpen && (backend.draft.signature_text || "").length > 0
            Layout.fillWidth: true; Layout.bottomMargin: 12
            text: backend.draft.signature_text || ""
            textFormat: Text.PlainText; wrapMode: Text.Wrap
            color: Theme.muted; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12)
            TapHandler { onTapped: { root.signatureOpen = true; signature.forceActiveFocus() } }
        }
        TextArea {
            id: signature
            objectName: "replySignature"
            property bool blocksWorkspaceShortcuts: true
            Layout.fillWidth: true
            visible: root.signatureOpen
            enabled: !backend.draftLocked
            textFormat: TextEdit.PlainText
            placeholderText: "Signature for this reply"
            placeholderTextColor: Theme.subtle
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            color: Theme.muted
            font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
            background: Rectangle { color: Theme.canvas; radius: 4; border.color: Theme.line }
            onTextChanged: if (!root.loading) backend.setDraftSignature(text)
            Accessible.name: "Signature for this reply"
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
                    width: Math.min(320, attachmentChips.width, chipName.implicitWidth + chipSize.implicitWidth + 72)
                    height: 30; radius: 5; color: "transparent"; border.color: Theme.controlBorder
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 6; spacing: 8
                        FmIcon { name: "attachment"; size: 14; color: Theme.secondary }
                        Text { id: chipName; Layout.fillWidth: true; text: attachmentChip.modelData.name; elide: Text.ElideRight; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
                        Text { id: chipSize; text: root.attachmentSize(attachmentChip.modelData.size); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
                        FmButton {
                            objectName: "removeDraftAttachment_" + attachmentChip.modelData.id
                            implicitWidth: 18; implicitHeight: 18; leftPadding: 0; rightPadding: 0
                            iconName: "close"; iconSize: 9
                            tip: "Remove retained attachment " + attachmentChip.modelData.name
                            enabled: !backend.draftLocked
                            onClicked: backend.setDraftAttachment(attachmentChip.modelData.id, false)
                        }
                    }
                }
            }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        RowLayout {
            visible: root.hasSelection
            Layout.fillWidth: true
            spacing: 1
            FmButton { text: "B"; font.bold: true; tip: "Bold selected text"; enabled: root.hasSelection && !backend.draftLocked; onClicked: document.format("bold") }
            FmButton { text: "I"; font.italic: true; tip: "Italic selected text"; enabled: root.hasSelection && !backend.draftLocked; onClicked: document.format("italic") }
            FmButton { text: "U"; font.underline: true; tip: "Underline selected text"; enabled: root.hasSelection && !backend.draftLocked; onClicked: document.format("underline") }
            FmButton { iconName: "undo"; tip: "Undo"; enabled: bodyEditor.canUndo && !backend.draftBusy; onClicked: document.undo() }
            FmButton { iconName: "redo"; tip: "Redo"; enabled: bodyEditor.canRedo && !backend.draftBusy; onClicked: document.redo() }
            Item { Layout.fillWidth: true }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.topMargin: 12; spacing: 6
            FmButton {
                objectName: "captureReply"
                text: backend.syntheticProfile ? "Capture locally" : "Send"
                prominent: true
                leftPadding: 16; rightPadding: 16
                font.pixelSize: Theme.fontSize(13)
                enabled: backend.syntheticProfile ? !backend.draftLocked && !backend.attachmentImport.pending && (document.plainText.trim().length > 0 || root.attachmentIds.length > 0) : backend.googleSend.can_send || false
                tip: backend.syntheticProfile ? "Record an outgoing MIME copy on this machine; no email is delivered" : "Save and submit this version through Google. Sending newer edits after an accepted version creates another email."
                onClicked: { if (backend.syntheticProfile) backend.captureDraft(); else backend.sendGoogleDraft() }
            }
            FmButton { objectName: "draftAttachButton"; text: "Attach"; iconName: "attachment"; outlined: true; leftPadding: 12; rightPadding: 12; font.weight: Font.Medium; font.pixelSize: Theme.fontSize(13); tip: "Attach a local file or a retained conversation file"; enabled: backend.connected && !backend.draftBusy && !backend.attachmentImportBusy; onClicked: attachmentMenu.popup() }
            FmButton { objectName: "draftDiscardButton"; text: "Discard"; leftPadding: 12; rightPadding: 12; font.pixelSize: Theme.fontSize(13); enabled: !backend.draftLocked && !backend.attachmentImport.pending; onClicked: discardDialog.open() }
            Item { Layout.fillWidth: true }
            Text {
                text: backend.draftStatus
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                color: text.indexOf("Not saved") >= 0 ? Theme.error : Theme.subtle
                wrapMode: Text.WordWrap
                font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
            }
        }
        RowLayout {
            visible: (backend.attachmentImport.status || "").length > 0
            Layout.fillWidth: true; spacing: 8
            Text { objectName: "draftAttachmentStatus"; Layout.fillWidth: true; text: backend.attachmentImport.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.attachmentImport.retry ? Theme.warn : text.indexOf("Not attached") === 0 ? Theme.error : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            FmButton { objectName: "retryDraftAttachment"; visible: backend.attachmentImport.retry || false; text: "Retry attachment"; outlined: true; enabled: backend.connected && !backend.attachmentImportBusy && !backend.draftBusy; onClicked: { backend.clearError(); backend.retryDraftAttachment() } }
        }
        ColumnLayout {
            visible: backend.googleSend.visible || false
            Layout.fillWidth: true; Layout.topMargin: visible ? 8 : 0; spacing: 6
            Text { objectName: "googleSendStatus"; Layout.fillWidth: true; text: backend.googleSend.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.googleSend.warning ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            Text { visible: (backend.googleSend.request_id || "").length > 0; text: "Request " + (backend.googleSend.request_id || ""); textFormat: Text.PlainText; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(11); Layout.fillWidth: true; wrapMode: Text.Wrap }
            Flow {
                Layout.fillWidth: true; spacing: 6
                FmButton { objectName: "checkGoogleSend"; text: backend.googleSend.check_label || "Check status"; outlined: true; visible: backend.googleSend.can_check || false; onClicked: backend.checkGoogleSend() }
                FmButton { objectName: "showCurrentGoogleSend"; text: "View current send"; visible: backend.googleSend.can_select_current || false; onClicked: backend.showCurrentGoogleSend() }
                FmButton { objectName: "resumeGoogleSend"; text: backend.googleSend.resume_label || "Send prepared version"; visible: backend.googleSend.can_resume || false; onClicked: backend.resumeGoogleSend() }
                FmButton { objectName: "abandonGoogleSend"; text: "Abandon prepared send"; visible: backend.googleSend.can_abandon || false; onClicked: backend.abandonGoogleSend() }
                FmButton { objectName: "cancelGoogleSend"; text: "Cancel"; visible: backend.googleSend.can_cancel || false; onClicked: backend.cancelGoogleSend() }
            }
        }
        ColumnLayout {
            visible: backend.googleDraft.visible || false
            Layout.fillWidth: true; Layout.topMargin: visible ? 8 : 0; spacing: 6
            Text { objectName: "googleDraftStatus"; Layout.fillWidth: true; text: backend.googleDraft.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.googleDraft.warning ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            RowLayout {
                visible: backend.googleDraft.can_check || backend.googleDraft.can_replace || backend.googleDraft.busy
                spacing: 6
                FmButton { objectName: "checkGoogleDraft"; text: "Check Google"; outlined: true; visible: !backend.googleDraft.busy; enabled: backend.googleDraft.can_check || false; onClicked: backend.reconcileGoogleDraft() }
                FmButton {
                    objectName: "replaceGoogleDraft"; text: "Replace Google copy…"; visible: backend.googleDraft.can_replace || false
                    enabled: backend.googleDraft.can_replace || false
                    onClicked: {
                        const context = backend.googleDraftReplacementContext()
                        if (!context.request_id) return
                        googleDraftDialog.capturedContext = context
                        googleDraftDialog.open()
                    }
                }
                FmButton { objectName: "cancelGoogleDraft"; text: "Cancel"; visible: backend.googleDraft.busy || false; onClicked: backend.cancelGoogleDraftOperation() }
            }
        }
        RowLayout {
            visible: backend.draftStatus.indexOf("Not saved") >= 0 && !backend.draftConflict
            FmButton { text: "Retry save"; outlined: true; enabled: !backend.draftLocked; onClicked: { backend.clearError(); backend.saveDraft() } }
            FmButton { text: "Reload saved draft…"; enabled: !backend.draftLocked; onClicked: reloadDialog.open() }
        }
        ColumnLayout {
            visible: backend.draftConflict
            Layout.fillWidth: true; spacing: 8
            Text { Layout.fillWidth: true; text: "Another interface saved a newer draft. Your body, formatting, signature and attachments are still here. Choose which version to keep."; color: Theme.error; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            RowLayout {
                FmButton { objectName: "saveMyDraftVersion"; text: "Save my version…"; outlined: true; enabled: !backend.draftBusy; onClicked: conflictDialog.open() }
                FmButton { text: "Reload saved draft…"; enabled: !backend.draftBusy; onClicked: reloadDialog.open() }
            }
        }
        Text {
            Layout.fillWidth: true
            visible: backend.captureStatus.length > 0
            text: backend.captureStatus
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Theme.muted
            font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11) }
        }
    }
    Menu {
        id: attachmentMenu
        objectName: "draftAttachmentMenu"
        MenuItem { objectName: "draftAttachLocalFile"; text: "Choose a local file…"; enabled: !backend.attachmentImportBusy; onTriggered: root.chooseLocalAttachment() }
        MenuSeparator { visible: root.availableAttachments.length > 0 }
        Repeater {
            model: root.availableAttachments
            MenuItem {
                required property var modelData
                text: modelData.name
                checkable: true
                checked: root.attachmentIds.indexOf(modelData.id) >= 0
                onTriggered: backend.setDraftAttachment(modelData.id, checked)
            }
        }
    }
    FileDialog {
        id: localAttachmentDialog
        objectName: "draftAttachmentFileDialog"
        property string targetAccount: ""
        property string targetDraft: ""
        title: "Attach a file"
        fileMode: FileDialog.OpenFile
        onAccepted: backend.importDraftAttachment(selectedFile, targetAccount, targetDraft)
    }
    Dialog {
        id: discardDialog
        implicitWidth: 480
        parent: Overlay.overlay
        anchors.centerIn: parent
        title: "Discard this draft?"
        modal: true
        standardButtons: Dialog.Discard | Dialog.Cancel
        contentItem: Label { property bool blocksWorkspaceShortcuts: true; text: "The saved body, signature and selected attachments will be cleared.\nReceived mail and local capture history stay available."; wrapMode: Text.WordWrap }
        onDiscarded: backend.discardDraft()
    }
    Dialog {
        id: reloadDialog
        implicitWidth: 480
        parent: Overlay.overlay
        anchors.centerIn: parent
        title: "Reload the saved draft?"
        modal: true
        standardButtons: Dialog.Ok | Dialog.Cancel
        contentItem: Label { property bool blocksWorkspaceShortcuts: true; text: "This replaces the unsaved text in this editor with the last saved draft."; wrapMode: Text.WordWrap }
        onAccepted: { backend.clearError(); backend.reloadDraft() }
    }
    Dialog {
        id: googleDraftDialog
        objectName: "googleDraftReplaceDialog"
        property var capturedContext: ({})
        implicitWidth: 480; parent: Overlay.overlay; anchors.centerIn: parent; modal: true
        title: "Replace the Google draft?"
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 16
            Text { Layout.fillWidth: true; text: "Upload this saved local version to Google. Your local version and any previously fetched Google copy stay retained. Another client can edit between the last check and the upload; that unseen edit can be overwritten. A changed local draft or known Google revision will require a new confirmation."; color: Theme.text; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                FmButton { objectName: "cancelReplaceGoogleDraft"; text: "Cancel"; onClicked: googleDraftDialog.close() }
                FmButton { objectName: "confirmReplaceGoogleDraft"; text: "Replace Google copy"; prominent: true; onClicked: { const context = googleDraftDialog.capturedContext; googleDraftDialog.close(); backend.replaceGoogleDraft(context) } }
            }
        }
    }
    Dialog {
        id: conflictDialog
        objectName: "draftConflictDialog"
        implicitWidth: 480; parent: Overlay.overlay; anchors.centerIn: parent; modal: true
        title: "Save your version?"
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 16
            Text { Layout.fillWidth: true; text: "This replaces the current saved draft with your retained body, formatting, signature and attachments. Another edit made while saving will cause a new conflict."; color: Theme.text; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                FmButton { text: "Cancel"; onClicked: conflictDialog.close() }
                FmButton { objectName: "confirmSaveMyDraft"; text: "Save my version"; prominent: true; onClicked: { conflictDialog.close(); backend.saveMyDraftVersion() } }
            }
        }
    }
}
