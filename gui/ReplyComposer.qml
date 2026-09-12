import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    signal dismissed()
    property bool loading: false
    property bool signatureOpen: false
    readonly property bool hasSelection: bodyEditor.selectionEnd > bodyEditor.selectionStart
    readonly property var attachmentIds: backend.draft.attachment_ids || []
    readonly property var availableAttachments: {
        let items = []
        for (const message of (backend.conversation.messages || []))
            for (const item of (message.attachments || [])) if (item.state === "available") items.push(item)
        return items
    }
    function focusEditor() { bodyEditor.forceActiveFocus() }
    function syncDraft() {
        loading = true
        document.load(backend.draft.body || "", backend.draft.body_html || "")
        if (signature.text !== (backend.draft.signature_text || "")) signature.text = backend.draft.signature_text || ""
        loading = false
    }
    color: Theme.surface
    border.color: Theme.controlBorder
    radius: 7
    implicitHeight: contents.implicitHeight + 28
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
        anchors { top: parent.top; left: parent.left; right: parent.right; margins: 14 }
        spacing: 10
        RowLayout {
            Layout.fillWidth: true
            FmIcon { name: "reply"; size: 15; color: Theme.muted }
            Text { text: "Reply"; color: Theme.text; font { family: Theme.fontFamily; pixelSize: 14; weight: Font.Medium } }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "collapseReply"; iconName: "chevronDown"; tip: "Collapse reply — saved text stays here"; onClicked: { backend.saveDraft(); root.dismissed() } }
        }
        Text {
            Layout.fillWidth: true
            text: "To  " + (backend.draft.to || "")
            color: Theme.muted; textFormat: Text.PlainText; elide: Text.ElideRight
            font { family: Theme.fontFamily; pixelSize: 12 }
        }
        Text {
            Layout.fillWidth: true
            text: "From  " + (backend.draft.from_name || "") + " <" + (backend.draft.from || "") + ">"
            color: Theme.subtle; textFormat: Text.PlainText; elide: Text.ElideRight
            font { family: Theme.fontFamily; pixelSize: 11 }
        }
        Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
        ScrollView {
            id: composerScroll
            objectName: "replyBodyScroll"
            Layout.fillWidth: true
            Layout.preferredHeight: Math.max(112, Math.min(300, bodyEditor.contentHeight + 16))
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        TextArea {
            id: bodyEditor
            objectName: "replyComposer"
            width: composerScroll.availableWidth
            enabled: !backend.draftLocked
            textFormat: TextEdit.RichText
            selectByMouse: true
            persistentSelection: true
            wrapMode: TextEdit.Wrap
            placeholderText: "Write your reply…"
            placeholderTextColor: Theme.subtle
            color: Theme.text
            selectionColor: Theme.accent
            selectedTextColor: Theme.canvas
            font { family: Theme.fontFamily; pixelSize: 13 }
            background: null
            padding: 0
            Accessible.name: "Reply body"
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
                MenuItem { text: "Select all"; onTriggered: bodyEditor.selectAll() }
            }
        }
        }
        TextArea {
            id: signature
            objectName: "replySignature"
            Layout.fillWidth: true
            visible: root.signatureOpen
            enabled: !backend.draftLocked
            textFormat: TextEdit.PlainText
            placeholderText: "Signature for this reply"
            placeholderTextColor: Theme.subtle
            selectByMouse: true
            wrapMode: TextEdit.Wrap
            color: Theme.muted
            font { family: Theme.fontFamily; pixelSize: 12 }
            background: Rectangle { color: Theme.canvas; radius: 4; border.color: Theme.line }
            onTextChanged: if (!root.loading) backend.setDraftSignature(text)
            Accessible.name: "Signature for this reply"
        }
        Flow {
            Layout.fillWidth: true
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
            Layout.fillWidth: true
            spacing: 1
            FmButton { text: "B"; font.bold: true; tip: "Bold selected text"; enabled: root.hasSelection && !backend.draftLocked; onClicked: document.format("bold") }
            FmButton { text: "I"; font.italic: true; tip: "Italic selected text"; enabled: root.hasSelection && !backend.draftLocked; onClicked: document.format("italic") }
            FmButton { text: "U"; font.underline: true; tip: "Underline selected text"; enabled: root.hasSelection && !backend.draftLocked; onClicked: document.format("underline") }
            FmButton { iconName: "undo"; tip: "Undo"; enabled: bodyEditor.canUndo && !backend.draftBusy; onClicked: document.undo() }
            FmButton { iconName: "redo"; tip: "Redo"; enabled: bodyEditor.canRedo && !backend.draftBusy; onClicked: document.redo() }
            FmButton { iconName: "attachment"; tip: "Attach a retained file from this conversation"; enabled: root.availableAttachments.length > 0 && !backend.draftBusy; onClicked: attachmentMenu.popup() }
            FmButton { text: "Signature"; selected: root.signatureOpen; onClicked: root.signatureOpen = !root.signatureOpen }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "discardReply"; iconName: "trash"; tip: "Discard this draft"; enabled: !backend.draftLocked; onClicked: discardDialog.open() }
        }
        RowLayout {
            Layout.fillWidth: true
            FmButton {
                objectName: "captureReply"
                text: "Capture locally"
                iconName: "arrowUp"
                prominent: true
                enabled: !backend.draftLocked && (document.plainText.trim().length > 0 || root.attachmentIds.length > 0)
                tip: "Record an outgoing MIME copy on this machine; no email is delivered"
                onClicked: backend.captureDraft()
            }
            Item { Layout.fillWidth: true }
            Text {
                text: backend.draftStatus
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                color: text.indexOf("Not saved") >= 0 ? Theme.error : Theme.subtle
                wrapMode: Text.WordWrap
                font { family: Theme.fontFamily; pixelSize: 11 }
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
            Text { Layout.fillWidth: true; text: "Another interface saved a newer draft. Your body, formatting, signature and attachments are still here. Choose which version to keep."; color: Theme.error; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pixelSize: 12 }
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
            font { family: Theme.fontFamily; pixelSize: 11 }
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
        title: "Discard this reply?"
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
            Text { Layout.fillWidth: true; text: "This replaces the current saved draft with your retained body, formatting, signature and attachments. Another edit made while saving will cause a new conflict."; color: Theme.text; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pixelSize: 13 }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                FmButton { text: "Cancel"; onClicked: conflictDialog.close() }
                FmButton { objectName: "confirmSaveMyDraft"; text: "Save my version"; prominent: true; onClicked: { conflictDialog.close(); backend.saveMyDraftVersion() } }
            }
        }
    }
}
