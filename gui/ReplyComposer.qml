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
    readonly property bool popupOpen: sourceHistory.opened || discardDialog.opened || reloadDialog.opened || conflictDialog.opened || googleDraftDialog.opened || attachmentMenu.opened || contextMenu.opened || localAttachmentDialog.visible
    readonly property string mode: backend.draft.mode || "reply"
    readonly property bool readOnly: backend.draftReadOnly
    property string snapshotContext: backend.account + "/" + backend.activeDraftId + "/" + ((backend.draft.remote_source || {}).source_revision || 0)
    onSnapshotContextChanged: sourceHistory.close()
    readonly property string draftTitle: readOnly ? "Google draft preview" : ({new:"New message",reply:"Reply",reply_all:"Reply all",forward:"Forward"})[mode] || "Reply"
    readonly property bool hasSelection: bodyEditor.selectionEnd > bodyEditor.selectionStart
    readonly property var attachmentIds: backend.draft.attachment_ids || []
    readonly property var availableAttachments: {
        let items = []
        for (const message of (backend.conversation.messages || []))
            for (const item of (message.attachments || [])) if (item.state === "available") items.push(item)
        return items
    }
    function sourceIssues() {
        const descriptions = {
            header_preview_truncated: "Original header preview is shortened",
            recipient_preview_limit: "Recipient preview is shortened",
            routing_preview_limit: "Routing preview is incomplete",
            unparsed_recipient: "Some recipients need the original source",
            unparsed_from: "From needs the original source",
            multiple_from_preview: "Multiple From addresses are retained in the source",
            from_preview_limit: "From preview is shortened",
            subject_preview_limit: "Subject preview is shortened",
            body_preview_limit: "Text preview is shortened",
            body_preview_unavailable: "Text preview is unavailable",
            attachment_preview_limit: "Some files remain only in the original source",
            attachment_decode_problem: "Some files could not be decoded",
            source_too_large: "Source exceeds the preview size limit",
            malformed_mime: "The source could not be decoded",
            not_found: "The latest Google source was unavailable"
        }
        let messages = []
        for (const issue of ((backend.draft.remote_source || {}).issues || [])) {
            const text = descriptions[issue] || "Some content needs the original source"
            if (messages.indexOf(text) < 0) messages.push(text)
        }
        return messages.join(" · ")
    }
    function focusEditor() { bodyEditor.forceActiveFocus() }
    function chooseLocalAttachment() {
        if (root.readOnly) return
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
        document.load(backend.draft.body || "", root.readOnly ? "" : (backend.draft.body_html || ""))
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
        readOnly: root.readOnly
        onContentChanged: if (!root.loading) backend.editFormattedDraft(plainText, html)
    }
    ColumnLayout {
        id: contents
        anchors { top: parent.top; left: parent.left; right: parent.right; leftMargin: 20 }
        spacing: 0
        RowLayout {
            Layout.fillWidth: true; Layout.bottomMargin: 14; spacing: 10
            Text { text: root.draftTitle; color: Theme.text; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(15); weight: Font.Medium; letterSpacing: -0.15 } }
            Text { visible: !root.readOnly && root.mode!=="new"; text: root.mode==="forward" ? "original message quoted below" : "in this conversation"; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12); Layout.fillWidth: true; elide: Text.ElideRight }
            Item { Layout.fillWidth: true }
            FmButton { objectName: "collapseReply"; text: "Minimise"; font.pixelSize: Theme.fontSize(12); leftPadding: 0; rightPadding: 0; tip: "Keep this draft and hide the editor"; onClicked: { backend.saveDraft(); root.dismissed() } }
            FmButton { objectName: "discardReply"; visible: !root.readOnly; text: "Discard"; font.pixelSize: Theme.fontSize(12); leftPadding: 4; rightPadding: 0; enabled: !backend.draftLocked && !backend.attachmentImport.pending; onClicked: discardDialog.open() }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        ColumnLayout {
            visible: root.readOnly
            Layout.fillWidth: true; Layout.bottomMargin: 12; spacing: 8
            Text {
                objectName: "draftSnapshotNotice"
                Layout.fillWidth: true
                text: ((backend.draft.remote_source || {}).source_state === "available" ? "Read-only preview · saved source " : "Earlier cached preview · latest source ") + ((backend.draft.remote_source || {}).source_revision || 0)
                    + " · " + ((backend.draft.remote_source || {}).source_state || "unavailable")
                    + ((backend.draft.remote_source || {}).preview_state !== "complete" ? " · incomplete text preview" : "")
                    + ((backend.draft.remote_source || {}).latest_scan_complete && !(backend.draft.remote_source || {}).seen_in_latest_scan
                        ? "\nNot seen in the latest completed check. This does not establish sending or deletion." : "")
                    + (((backend.draft.remote_source || {}).issues || []).length ? "\n" + root.sourceIssues() : "")
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12)
            }
            ScrollView {
                id: snapshotHeaderScroll
                Layout.fillWidth: true; Layout.preferredHeight: Math.min(200, snapshotHeaders.implicitHeight)
                contentWidth: availableWidth; clip: true
                ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                ColumnLayout {
                    id: snapshotHeaders; width: snapshotHeaderScroll.availableWidth; spacing: 5
                    Repeater {
                        model: backend.draftSnapshotHeaders
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true; spacing: 10
                            Text { text: modelData.label; Layout.preferredWidth: 54; Layout.alignment: Qt.AlignTop; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
                            TextArea {
                                objectName: "draftSnapshotHeader_" + modelData.label
                                property bool blocksWorkspaceShortcuts: true
                                Layout.fillWidth: true; padding: 0; background: null
                                readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap
                                text: modelData.value; placeholderText: "Not present in source"; placeholderTextColor: Theme.subtle
                                color: Theme.text; selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
                                font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13)
                            }
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true; spacing: 8
                FmButton { objectName: "draftSourceHistoryButton"; text: "Saved sources"; outlined: true; onClicked: { backend.loadDraftSources(); sourceHistory.open() } }
                Text { Layout.fillWidth: true; text: "Original MIME, HTML and files are retained. This preview cannot be edited or sent."; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            }
        }
        RowLayout {
            visible: !root.readOnly
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "From"; Layout.preferredWidth: 44; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            Text { text: (backend.draft.from_name || backend.currentAccount.name || "")+" · "+(backend.draft.from || ""); color: Theme.text; textFormat: Text.PlainText; elide: Text.ElideRight; Layout.fillWidth: true; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        RecipientField { visible: !root.readOnly; Layout.fillWidth: true; label: "To"; fieldName: "to_recipients"; showLinks: true; onCcRequested: root.ccOpen=true; onBccRequested: root.bccOpen=true }
        RecipientField { Layout.fillWidth: true; label: "Cc"; fieldName: "cc_recipients"; visible: !root.readOnly && (root.ccOpen || (backend.draft.cc_recipients||[]).length>0) }
        RecipientField { Layout.fillWidth: true; label: "Bcc"; fieldName: "bcc_recipients"; visible: !root.readOnly && (root.bccOpen || (backend.draft.bcc_recipients||[]).length>0) }
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "Subject"; Layout.preferredWidth: 44; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
            TextField { objectName: "draftSubject"; property bool blocksWorkspaceShortcuts: true; Layout.fillWidth: true; padding: 0; text: backend.draft.subject || ""; placeholderText: "Subject"; color: Theme.text; placeholderTextColor: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(14); font.weight: Font.Medium; background: Item {} enabled: root.readOnly || !backend.draftLocked; readOnly: root.readOnly; selectByMouse: true; onTextEdited: backend.setDraftSubject(text) }
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
            enabled: root.readOnly || !backend.draftLocked
            readOnly: root.readOnly
            textFormat: TextEdit.RichText
            selectByMouse: true
            persistentSelection: true
            wrapMode: TextEdit.Wrap
            placeholderText: root.readOnly ? "No text preview available. The original source is retained." : "Write it yourself — nothing here drafts for you."
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
                MenuItem { text: "Cut"; enabled: !root.readOnly && root.hasSelection; onTriggered: bodyEditor.cut() }
                MenuItem { text: "Paste plain text"; enabled: !root.readOnly; onTriggered: document.pastePlain() }
                MenuSeparator {}
                MenuItem { text: "Bold"; enabled: !root.readOnly && root.hasSelection; onTriggered: document.format("bold") }
                MenuItem { text: "Italic"; enabled: !root.readOnly && root.hasSelection; onTriggered: document.format("italic") }
                MenuItem { text: "Underline"; enabled: !root.readOnly && root.hasSelection; onTriggered: document.format("underline") }
                MenuItem { text: "Edit signature"; enabled: !root.readOnly; onTriggered: root.signatureOpen = true }
                MenuSeparator {}
                MenuItem { text: "Select all"; onTriggered: bodyEditor.selectAll() }
            }
        }
        }
        Text {
            visible: !root.readOnly && !root.signatureOpen && (backend.draft.signature_text || "").length > 0
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
            visible: !root.readOnly && root.signatureOpen
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
                    width: Math.min(320, attachmentChips.width, chipName.implicitWidth + chipSize.implicitWidth + (root.readOnly ? 104 : 72))
                    height: 30; radius: 5; color: "transparent"; border.color: Theme.controlBorder
                    RowLayout {
                        anchors.fill: parent; anchors.leftMargin: 10; anchors.rightMargin: 6; spacing: 8
                        FmIcon { name: "attachment"; size: 14; color: Theme.secondary }
                        Text { id: chipName; Layout.fillWidth: true; text: attachmentChip.modelData.name; elide: Text.ElideRight; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
                        Text { id: chipSize; text: root.attachmentSize(attachmentChip.modelData.size); color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
                        FmButton {
                            objectName: "exportDraftAttachment_" + attachmentChip.modelData.id
                            visible: root.readOnly; text: "Copy"; implicitHeight: 24; leftPadding: 2; rightPadding: 2
                            enabled: !backend.draftSources.exporting && (backend.draft.remote_source || {}).source_state === "available"
                            tip: (backend.draft.remote_source || {}).source_state === "available" ? "Export the exact retained file; no external content is fetched" : "Use Saved sources to export an earlier version"
                            onClicked: { backend.exportDraftSource((backend.draft.remote_source || {}).source_revision, attachmentChip.modelData.id); sourceHistory.open() }
                        }
                        FmButton {
                            objectName: "removeDraftAttachment_" + attachmentChip.modelData.id
                            visible: !root.readOnly
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
            visible: !root.readOnly && root.hasSelection
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
                visible: !root.readOnly
                text: backend.syntheticProfile ? "Capture locally" : "Send"
                prominent: true
                leftPadding: 16; rightPadding: 16
                font.pixelSize: Theme.fontSize(13)
                enabled: backend.syntheticProfile ? !backend.draftLocked && !backend.attachmentImport.pending && (document.plainText.trim().length > 0 || root.attachmentIds.length > 0) : backend.googleSend.can_send || false
                tip: backend.syntheticProfile ? "Record an outgoing MIME copy on this machine; no email is delivered" : "Save and submit this version through Google. Sending newer edits after an accepted version creates another email."
                onClicked: { if (backend.syntheticProfile) backend.captureDraft(); else backend.sendGoogleDraft() }
            }
            FmButton { objectName: "draftAttachButton"; visible: !root.readOnly; text: "Attach"; iconName: "attachment"; outlined: true; leftPadding: 12; rightPadding: 12; font.weight: Font.Medium; font.pixelSize: Theme.fontSize(13); tip: "Attach a local file or a retained conversation file"; enabled: backend.connected && !backend.draftBusy && !backend.attachmentImportBusy; onClicked: attachmentMenu.popup() }
            FmButton { objectName: "draftDiscardButton"; visible: !root.readOnly; text: "Discard"; leftPadding: 12; rightPadding: 12; font.pixelSize: Theme.fontSize(13); enabled: !backend.draftLocked && !backend.attachmentImport.pending; onClicked: discardDialog.open() }
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
            visible: !root.readOnly && (backend.attachmentImport.status || "").length > 0
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
                visible: backend.googleDraft.can_check || backend.googleDraft.can_replace || backend.googleDraft.busy || false
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
    Popup {
        id: sourceHistory
        objectName: "draftSourceHistoryPopup"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(560, parent ? parent.width - 32 : 560)
        height: Math.min(470, parent ? parent.height - 48 : 470)
        modal: true; focus: true; padding: 20
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 10 }
        contentItem: ColumnLayout {
            property bool blocksWorkspaceShortcuts: true
            spacing: 10
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Saved Google draft sources"; Layout.fillWidth: true; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(15); wrapMode: Text.Wrap }
                FmButton { objectName: "closeDraftSourceHistory"; text: "Close"; onClicked: sourceHistory.close() }
            }
            Text { Layout.fillWidth: true; text: "Export makes a verified local copy. No external content is fetched."; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            ScrollView {
                id: sourceScroll
                Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                contentWidth: availableWidth; ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
                ColumnLayout {
                    width: sourceScroll.availableWidth; spacing: 10
                    Repeater {
                        model: backend.draftSources.sources || []
                        ColumnLayout {
                            id: sourceVersion
                            required property var modelData
                            Layout.fillWidth: true; spacing: 4
                            RowLayout {
                                Layout.fillWidth: true
                                Text { Layout.fillWidth: true; text: "Source " + sourceVersion.modelData.source_revision + " · " + sourceVersion.modelData.source_state; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(13) }
                                FmButton {
                                    objectName: "exportDraftSource_" + sourceVersion.modelData.source_revision
                                    text: "Export source"; outlined: true
                                    enabled: !backend.draftSources.exporting && sourceVersion.modelData.source_state === "available"
                                    onClicked: backend.exportDraftSource(sourceVersion.modelData.source_revision, "")
                                }
                            }
                            Flow {
                                Layout.fillWidth: true; spacing: 4
                                visible: sourceVersion.modelData.source_state === "available"
                                Repeater {
                                    model: sourceVersion.modelData.attachment_ids || []
                                    FmButton {
                                        required property string modelData
                                        required property int index
                                        objectName: "exportHistoricalDraftAttachment_" + sourceVersion.modelData.source_revision + "_" + modelData
                                        text: "Copy file " + (index + 1); outlined: true; font.pixelSize: Theme.fontSize(12)
                                        tip: "Export this retained file from source " + sourceVersion.modelData.source_revision
                                        enabled: !backend.draftSources.exporting
                                        onClicked: backend.exportDraftSource(sourceVersion.modelData.source_revision, modelData)
                                    }
                                }
                            }
                        }
                    }
                    FmButton { text: "Latest sources"; visible: (backend.draftSources.offset || 0) > 0; enabled: !backend.draftSources.loading; onClicked: backend.loadDraftSources() }
                    FmButton {
                        objectName: "moreDraftSources"; text: "Older sources"
                        visible: backend.draftSources.next_offset !== undefined && backend.draftSources.next_offset !== null
                        enabled: !backend.draftSources.loading
                        onClicked: backend.loadDraftSources(true)
                    }
                }
            }
            Text { objectName: "draftSourceExportStatus"; Layout.fillWidth: true; text: backend.draftSources.status || (backend.draftSources.loading ? "Loading saved sources…" : ""); textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pixelSize: Theme.fontSize(12) }
            FmButton { objectName: "showDraftSourceFolder"; text: "Show copy in folder"; visible: ((backend.draftSources.export || {}).path || "").length > 0; onClicked: backend.showDraftSourceFolder() }
        }
    }
    Menu {
        id: attachmentMenu
        objectName: "draftAttachmentMenu"
        MenuItem { objectName: "draftAttachLocalFile"; text: "Choose a local file…"; enabled: !root.readOnly && !backend.attachmentImportBusy; onTriggered: root.chooseLocalAttachment() }
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
