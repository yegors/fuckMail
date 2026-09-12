import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    signal dismissed()
    property bool loading: false
    property bool signatureOpen: false
    property bool ccOpen: false
    property bool bccOpen: false
    readonly property bool popupOpen: discardDialog.opened || reloadDialog.opened || conflictDialog.opened || attachmentMenu.opened || contextMenu.opened
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
    function confirmDiscard() { if (!backend.draftLocked) discardDialog.open() }
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
            FmButton { objectName: "discardReply"; text: "Discard"; font.pixelSize: Theme.fontSize(12); leftPadding: 4; rightPadding: 0; enabled: !backend.draftLocked; onClicked: discardDialog.open() }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "From"; Layout.preferredWidth: 44; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            Text { text: (backend.draft.from_name || backend.currentAccount.name || "")+" · "+(backend.draft.from || ""); color: Theme.text; textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        RecipientField { Layout.fillWidth: true; label: "To"; fieldName: "to_recipients"; showLinks: true; onCcRequested: root.ccOpen=true; onBccRequested: root.bccOpen=true }
        RecipientField { Layout.fillWidth: true; label: "Cc"; fieldName: "cc_recipients"; visible: root.ccOpen || (backend.draft.cc_recipients||[]).length>0 }
        RecipientField { Layout.fillWidth: true; label: "Bcc"; fieldName: "bcc_recipients"; visible: root.bccOpen || (backend.draft.bcc_recipients||[]).length>0 }
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "Subject"; Layout.preferredWidth: 44; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            TextField { objectName: "draftSubject"; property bool blocksWorkspaceShortcuts: true; Layout.fillWidth: true; padding: 0; text: backend.draft.subject || ""; placeholderText: "Subject"; color: Theme.text; placeholderTextColor: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); font.weight: Font.Medium; background: Item {} enabled: !backend.draftLocked; selectByMouse: true; onTextEdited: backend.setDraftSubject(text) }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
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
            visible: (backend.draft.attachments||[]).length>0
            Layout.fillWidth: true
            Layout.bottomMargin: 12
            spacing: 5
            Repeater {
                model: backend.draft.attachments || []
                FmButton {
                    required property var modelData
                    text: modelData.name
                    iconName: "attachment"
                    outlined: true
                    tip: "Remove retained attachment " + modelData.name
                    enabled: !backend.draftLocked
                    onClicked: backend.setDraftAttachment(modelData.id, false)
                }
            }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
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
                text: "Capture locally"
                prominent: true
                font.pixelSize: Theme.fontSize(13)
                enabled: !backend.draftLocked && (document.plainText.trim().length > 0 || root.attachmentIds.length > 0)
                tip: "Record an outgoing MIME copy on this machine; no email is delivered"
                onClicked: backend.captureDraft()
            }
            FmButton { text: "Attach"; iconName: "attachment"; outlined: true; font.pixelSize: Theme.fontSize(13); tip: root.availableAttachments.length ? "Attach a retained file from this conversation" : "No retained files in this conversation"; enabled: root.availableAttachments.length > 0 && !backend.draftBusy; onClicked: attachmentMenu.popup() }
            FmButton { text: "Discard"; font.pixelSize: Theme.fontSize(13); enabled: !backend.draftLocked; onClicked: discardDialog.open() }
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
