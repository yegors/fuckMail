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
    readonly property bool popupOpen: fromMenu.opened || toField.popupOpen || ccField.popupOpen || bccField.popupOpen || sourceActions.opened || sourceActionDialog.opened || deleteAbandonDialog.opened || sourceHistory.opened || discardDialog.opened || reloadDialog.opened || conflictDialog.opened || googleDraftDialog.opened || attachmentMenu.opened || contextMenu.opened || localAttachmentDialog.visible
    readonly property string mode: backend.draft.mode || "reply"
    readonly property bool snapshotReadOnly: backend.draftReadOnly
    readonly property bool readOnly: snapshotReadOnly || backend.accountArchived || backend.unsavedAccountBuffer
    property string snapshotContext: backend.account + "/" + backend.activeDraftId + "/" + ((backend.draft.remote_source || {}).source_revision || 0)
    onSnapshotContextChanged: { fromMenu.close(); sourceHistory.close(); sourceActions.close(); sourceActionDialog.close(); deleteAbandonDialog.close() }
    function confirmSourceAction(action) {
        const context = backend.draftSourceActionContext(action)
        if (!context.request_id) return
        sourceActions.close()
        sourceActionDialog.capturedContext = context
        sourceActionDialog.open()
    }
    function inspectDeletion(requestId) { sourceHistory.close(); backend.loadDraftDeletion(requestId) }
    readonly property string draftTitle: backend.unsavedAccountBuffer ? "Retained unsaved edits" : backend.accountArchived ? "Archived draft" : backend.draft.transferred ? "Moved draft" : snapshotReadOnly ? "Google draft preview" : ({new:"New message",reply:"Reply",reply_all:"Reply all",forward:"Forward"})[mode] || "Reply"
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
    function confirmDiscard() {
        if (backend.draftLocked || backend.draftBusy || backend.attachmentImport.pending) return
        const draft = backend.draft
        const hasContent = (draft.subject || "").trim().length || (draft.body || "").trim().length
            || (draft.to_recipients || []).length || (draft.cc_recipients || []).length || (draft.bcc_recipients || []).length
            || (draft.attachment_ids || []).length || Object.keys(backend.recipientInputs).length
        discardDialog.capturedContext = backend.draftDiscardContext()
        if (hasContent) discardDialog.open(); else backend.discardDraft(discardDialog.capturedContext)
    }
    function syncDraft() {
        loading = true
        document.setIdentity(backend.account + "/" + (backend.activeDraftId || backend.selectedId))
        document.load(backend.draft.body || "", root.snapshotReadOnly ? "" : (backend.draft.body_html || ""))
        if (signature.text !== (backend.draft.signature_text || "")) signature.text = backend.draft.signature_text || ""
        loading = false
    }
    color: "transparent"
    implicitHeight: contents.implicitHeight
    Rectangle { width: 2; height: parent.height; color: fileDrop.containsDrag ? Theme.accent : Theme.accentAlpha(0.55) }
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
            Text { anchors.centerIn: parent; text: "Drop files to attach"; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(16) }
        }
    }
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
            Text { text: root.draftTitle; color: Theme.text; font { family: Theme.fontFamily; pointSize: Theme.fontPoints(15); weight: Font.Medium; letterSpacing: -0.15 } }
            Item { Layout.fillWidth: true }
            RowLayout {
                spacing: 6
                FmButton { objectName: "discardReply"; visible: !root.readOnly; iconName: "trash"; iconSize: 12; implicitWidth: 26; implicitHeight: 26; leftPadding: 0; rightPadding: 0; outlined: true; foreground: !enabled ? Theme.subtle : hovered ? Theme.error : Theme.inkAlpha(.6); tip: "Discard this draft"; enabled: !backend.draftLocked && !backend.draftBusy && !backend.attachmentImport.pending; onClicked: root.confirmDiscard() }
                FmButton { objectName: "collapseReply"; iconName: "minus"; iconSize: 12; implicitWidth: 26; implicitHeight: 26; leftPadding: 0; rightPadding: 0; outlined: true; foreground: hovered ? Theme.text : Theme.inkAlpha(.6); tip: "Minimise — keeps the draft, Esc"; onClicked: { if (backend.accountNavigationAllowed()) { backend.saveDraft(); root.dismissed() } } }
            }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        Text {
            objectName: "archivedDraftNotice"; visible: backend.accountArchived || backend.unsavedAccountBuffer
            Layout.fillWidth: true; wrapMode: Text.Wrap; textFormat: Text.PlainText
            text: backend.unsavedAccountBuffer ? backend.draftStatus : "Saved account archive · selectable preview. Editing, autosave, sending and provider checks are disabled."
            color: backend.unsavedAccountBuffer ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13)
        }
        ColumnLayout {
            visible: root.readOnly
            Layout.fillWidth: true; Layout.bottomMargin: 12; spacing: 8
            Text {
                objectName: "draftSnapshotNotice"
                visible: root.snapshotReadOnly && !backend.draft.transferred
                Layout.fillWidth: true
                text: ((backend.draft.remote_source || {}).source_state === "available" ? "Read-only preview · saved source " : "Earlier cached preview · latest source ") + ((backend.draft.remote_source || {}).source_revision || 0)
                    + " · " + ((backend.draft.remote_source || {}).source_state || "unavailable")
                    + ((backend.draft.remote_source || {}).preview_state !== "complete" ? " · incomplete text preview" : "")
                    + ((backend.draft.remote_source || {}).latest_scan_complete && !(backend.draft.remote_source || {}).seen_in_latest_scan
                        ? "\nNot seen in the latest completed check. This does not establish sending or deletion." : "")
                    + (((backend.draft.remote_source || {}).issues || []).length ? "\n" + root.sourceIssues() : "")
                textFormat: Text.PlainText; wrapMode: Text.Wrap
                color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            }
            ScrollView {
                ScrollBar.vertical: FmScrollBar {}
                id: snapshotHeaderScroll
                visible: !backend.draft.transferred
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
                            Text { text: modelData.label; Layout.preferredWidth: 54; Layout.alignment: Qt.AlignTop; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
                            TextArea {
                                objectName: "draftSnapshotHeader_" + modelData.label
                                property bool blocksWorkspaceShortcuts: true
                                Layout.fillWidth: true; padding: 0; background: null
                                readOnly: true; selectByMouse: true; textFormat: TextEdit.PlainText; wrapMode: TextEdit.Wrap
                                text: modelData.value; placeholderText: "Not present in source"; placeholderTextColor: Theme.subtle
                                color: Theme.text; selectionColor: Theme.selection; selectedTextColor: Theme.selectedText
                                font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13)
                            }
                        }
                    }
                }
            }
            RowLayout {
                visible: root.snapshotReadOnly && !backend.draft.transferred
                Layout.fillWidth: true; spacing: 8
                FmButton { objectName: "draftSourceHistoryButton"; text: "Saved sources"; outlined: true; onClicked: { backend.loadDraftSources(); sourceHistory.open() } }
                FmButton { objectName: "draftSourceActionsButton"; text: "Actions"; visible: !backend.accountArchived && !backend.unsavedAccountBuffer; outlined: true; onClicked: { backend.loadDraftAuthoring(); sourceActions.open() } }
                Text { Layout.fillWidth: true; text: "Original MIME, HTML and files are retained. This preview cannot be edited or sent."; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            }
        }
        ColumnLayout {
            visible: !root.snapshotReadOnly && (((backend.draft.remote_source || {}).source_revision || 0) > 0 || (backend.draftAuthoring.original_draft_id || "").length > 0)
            Layout.fillWidth: true; Layout.bottomMargin: 12; spacing: 5
            Text { Layout.fillWidth: true; text: "Original Google source retained"; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            Flow {
                Layout.fillWidth: true; spacing: 6
                FmButton { objectName: "authoredDraftSourceHistory"; text: "Saved sources"; outlined: true; visible: ((backend.draft.remote_source || {}).source_revision || 0) > 0; onClicked: { backend.loadDraftSources(); sourceHistory.open() } }
                FmButton { objectName: "openOriginalDraftSource"; text: "Open original source"; visible: (backend.draftAuthoring.original_draft_id || "").length > 0; onClicked: backend.openDraft(backend.draftAuthoring.original_draft_id) }
            }
        }
        ColumnLayout {
            visible: root.readOnly && ((backend.draftAuthoring.status || "").length > 0 || backend.draftAuthoring.retry || (backend.draftAuthoring.result_draft_id || "").length > 0)
            Layout.fillWidth: true; Layout.bottomMargin: 12; spacing: 5
            Text { objectName: "draftSourceActionStatus"; Layout.fillWidth: true; text: backend.draftAuthoring.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            Flow {
                Layout.fillWidth: true; spacing: 6
                FmButton { objectName: "retryDraftSourceAction"; text: "Retry same action"; visible: backend.draftAuthoring.retry || false; enabled: !backend.draftAuthoring.busy; onClicked: backend.retryDraftSourceAction() }
                FmButton { objectName: "openDraftSourceActionResult"; text: "Open saved draft"; visible: (backend.draftAuthoring.result_draft_id || "").length > 0; onClicked: backend.openDraftSourceActionResult() }
            }
        }
        RowLayout {
            visible: !root.readOnly
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "From"; Layout.preferredWidth: 44; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5) }
            Button {
                id: fromButton; objectName: "draftFromAccount"
                Layout.fillWidth: true; Layout.maximumWidth: contentItem.implicitWidth + 20
                implicitHeight: 30; leftPadding: 0; rightPadding: 4
                enabled: backend.draftTransfer.can_change && backend.accounts.length > 1
                Accessible.name: "From account, " + (backend.draft.from || "")
                onClicked: fromMenu.open()
                background: Rectangle { x: -6; width: fromButton.width + 8; color: fromButton.hovered || fromButton.down ? Theme.hover : "transparent"; radius: 4; border.color: fromButton.activeFocus ? Theme.accent : "transparent" }
                contentItem: RowLayout {
                    spacing: 6
                    Text { Layout.fillWidth: true; text: (backend.draft.from_name || backend.currentAccount.name || "")+" · "+(backend.draft.from || ""); color: fromButton.enabled ? Theme.text : Theme.subtle; textFormat: Text.PlainText; elide: Text.ElideRight; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
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
                            background: Rectangle { color: fromOption.highlighted || fromOption.hovered ? Theme.hover : "transparent"; radius: 4 }
                            contentItem: RowLayout {
                                spacing: 8
                                FmIcon { name: "check"; size: 14; color: Theme.text; opacity: fromOption.checked ? 1 : 0 }
                                Text { Layout.fillWidth: true; text: fromOption.text; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
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
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        ColumnLayout {
            Layout.fillWidth: true; spacing: 6
            visible: (backend.draftTransfer.status || "").length > 0
            Text { objectName: "draftTransferStatus"; Layout.fillWidth: true; text: backend.draftTransfer.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
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
        Text {
            objectName: "draftRecipientError"; visible: !root.readOnly && backend.recipientError.length > 0; Layout.fillWidth: true
            text: backend.recipientError; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.error; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
        }
        RowLayout {
            Layout.fillWidth: true; Layout.preferredHeight: 38; spacing: 10
            Text { text: "Subject"; Layout.preferredWidth: 44; color: Theme.inkAlpha(.45); font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5) }
            TextField { objectName: "draftSubject"; property bool blocksWorkspaceShortcuts: true; Layout.fillWidth: true; padding: 0; leftPadding: 0; rightPadding: 0; text: backend.draft.subject || ""; placeholderText: "Subject"; color: Theme.text; placeholderTextColor: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(14); font.weight: Font.Medium; background: Item {} enabled: root.readOnly || !backend.draftLocked; readOnly: root.readOnly; selectByMouse: true; onTextEdited: backend.setDraftSubject(text) }
        }
        Item { Layout.fillWidth: true; implicitHeight: 1; Rectangle { y: Theme.snapY(parent, 0); width: parent.width; height: Theme.ruleWidth; color: Theme.line } }
        ScrollView {
                ScrollBar.vertical: FmScrollBar {}
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
            font { family: Theme.fontFamily; pointSize: Theme.fontPoints(15) }
            background: null
            padding: 0; leftPadding: 0; rightPadding: 0
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
            visible: !root.snapshotReadOnly && !root.signatureOpen && (backend.draft.signature_text || "").length > 0
            Layout.fillWidth: true; Layout.bottomMargin: 12
            text: backend.draft.signature_text || ""
            textFormat: Text.PlainText; wrapMode: Text.Wrap
            color: Theme.muted; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12)
            TapHandler { enabled: !root.readOnly; onTapped: { root.signatureOpen = true; signature.forceActiveFocus() } }
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
            font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) }
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
                        Text { id: chipName; Layout.fillWidth: true; text: attachmentChip.modelData.name; elide: Text.ElideRight; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
                        Text { id: chipSize; text: root.attachmentSize(attachmentChip.modelData.size); color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                        FmButton {
                            objectName: "exportDraftAttachment_" + attachmentChip.modelData.id
                            visible: root.readOnly; text: root.snapshotReadOnly ? "Copy" : "Open"; implicitHeight: 24; leftPadding: 2; rightPadding: 2
                            enabled: root.snapshotReadOnly ? !backend.draftSources.exporting && (backend.draft.remote_source || {}).source_state === "available" : attachmentChip.modelData.state === "available"
                            tip: (backend.draft.remote_source || {}).source_state === "available" ? "Export the exact retained file; no external content is fetched" : "Use Saved sources to export an earlier version"
                            onClicked: { if (root.snapshotReadOnly) { backend.exportDraftSource((backend.draft.remote_source || {}).source_revision, attachmentChip.modelData.id); sourceHistory.open() } else backend.openAttachment(attachmentChip.modelData.id) }
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
                text: backend.syntheticProfile ? "Capture locally" : backend.googleSend.busy ? "Sending…" : "Send"
                prominent: true
                leftPadding: 16; rightPadding: 16
                font.pointSize: Theme.fontPoints(13)
                enabled: backend.syntheticProfile ? !backend.draftLocked && !backend.attachmentImport.pending && (document.plainText.trim().length > 0 || root.attachmentIds.length > 0) : backend.googleSend.can_send || false
                tip: backend.syntheticProfile ? "Record an outgoing MIME copy on this machine; no email is delivered" : "Save and send through Google."
                onClicked: { if (backend.syntheticProfile) backend.captureDraft(); else backend.sendGoogleDraft() }
            }
            FmButton { objectName: "draftAttachButton"; visible: !root.readOnly; text: "Attach"; iconName: "attachment"; outlined: true; leftPadding: 12; rightPadding: 12; font.weight: Font.Medium; font.pointSize: Theme.fontPoints(13); tip: "Attach a local file or a retained conversation file"; enabled: backend.connected && !backend.draftBusy && !backend.attachmentImportBusy; onClicked: attachmentMenu.popup() }
            Item { Layout.fillWidth: true }
            Text {
                text: backend.draftStatus
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignRight
                color: text.indexOf("Not saved") >= 0 ? Theme.error : Theme.subtle
                wrapMode: Text.WordWrap
                font { family: Theme.fontFamily; pointSize: Theme.fontPoints(12) }
            }
        }
        RowLayout {
            visible: !root.readOnly && (backend.attachmentImport.status || "").length > 0
            Layout.fillWidth: true; spacing: 8
            Text { objectName: "draftAttachmentStatus"; Layout.fillWidth: true; text: backend.attachmentImport.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.attachmentImport.retry ? Theme.warn : text.indexOf("Not attached") === 0 ? Theme.error : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmButton { objectName: "retryDraftAttachment"; visible: backend.attachmentImport.retry || backend.attachmentImport.can_resume || false; text: backend.attachmentImport.retry ? "Retry attachment" : "Continue"; outlined: true; enabled: backend.connected && !backend.attachmentImportBusy && !backend.draftBusy; onClicked: { backend.clearError(); backend.retryDraftAttachment() } }
            FmButton { objectName: "skipQueuedAttachments"; visible: (backend.attachmentImport.queued || 0) > 0; text: "Skip remaining"; onClicked: backend.skipQueuedDraftAttachments() }
        }
        ColumnLayout {
            visible: backend.googleSend.visible || false
            Layout.fillWidth: true; Layout.topMargin: visible ? 8 : 0; spacing: 6
            Text { objectName: "googleSendStatus"; Layout.fillWidth: true; text: backend.googleSend.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.googleSend.warning ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
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
            Layout.fillWidth: true; spacing: 6
            Text { Layout.fillWidth: true; text: backend.draftDiscard.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.warn; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            RowLayout {
                spacing: 6
                FmButton { objectName: "checkDraftDiscard"; text: "Check discard"; visible: backend.draftDiscard.can_check || false; onClicked: backend.checkDraftDiscard() }
                FmButton { objectName: "retryDraftDiscard"; text: backend.draftDiscard.retry_label || "Retry discard"; visible: backend.draftDiscard.can_retry || false; onClicked: backend.retryDraftDiscard() }
            }
        }
        ColumnLayout {
            visible: backend.googleDraft.visible || false
            Layout.fillWidth: true; Layout.topMargin: visible ? 8 : 0; spacing: 6
            Text { objectName: "googleDraftStatus"; Layout.fillWidth: true; text: backend.googleDraft.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.googleDraft.warning ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
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
            Text { Layout.fillWidth: true; text: "Another interface saved a newer draft. Your body, formatting, signature and attachments are still here. Choose which version to keep."; color: Theme.error; wrapMode: Text.Wrap; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            RowLayout {
                FmButton { objectName: "saveMyDraftVersion"; text: "Save my version…"; outlined: true; enabled: !backend.draftBusy; onClicked: conflictDialog.open() }
                FmButton { text: "Reload saved draft…"; enabled: !backend.draftBusy; onClicked: reloadDialog.open() }
            }
        }
        ColumnLayout {
            visible: backend.draftDeletion.visible || false
            Layout.fillWidth: true; Layout.topMargin: 8; spacing: 6
            Text { objectName: "draftDeletionStatus"; Layout.fillWidth: true; text: backend.draftDeletion.status || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: backend.draftDeletion.warning ? Theme.warn : Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            Flow {
                Layout.fillWidth: true; spacing: 6
                FmButton { objectName: "readDraftDeletion"; text: "Read saved receipt"; enabled: backend.draftDeletion.can_status || false; onClicked: backend.loadDraftDeletion() }
                FmButton { objectName: "checkDraftDeletion"; text: "Check Google"; enabled: backend.draftDeletion.can_check || false; onClicked: backend.checkDraftDeletion() }
                FmButton { objectName: "abandonDraftDeletion"; text: "Abandon prepared deletion…"; visible: backend.draftDeletion.can_abandon || false; onClicked: { const context = backend.draftDeletionAbandonContext(); if (context.request_id) { deleteAbandonDialog.capturedContext = context; deleteAbandonDialog.open() } } }
            }
        }
        Text {
            Layout.fillWidth: true
            visible: backend.captureStatus.length > 0
            text: backend.captureStatus
            textFormat: Text.PlainText
            wrapMode: Text.WordWrap
            color: Theme.muted
            font { family: Theme.fontFamily; pointSize: Theme.fontPoints(11) }
        }
    }
    Popup {
        id: sourceActions
        objectName: "draftSourceActionsPopup"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(400, parent ? parent.width - 32 : 400)
        padding: 16; modal: true; focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        contentItem: ColumnLayout {
            spacing: 6
            Text { Layout.fillWidth: true; text: backend.draftAuthoring.loading ? "Checking the complete saved source…" : "Actions for this saved Google source"; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmButton { objectName: "copyGoogleDraftSource"; Layout.fillWidth: true; text: "Create plain-text copy…"; textAlignment: Text.AlignLeft; enabled: backend.draftAuthoring.copy_allowed || false; onClicked: root.confirmSourceAction("copy") }
            Text { Layout.fillWidth: true; visible: !backend.draftAuthoring.copy_allowed && (backend.draftAuthoring.copy_reason || "").length > 0; text: backend.draftAuthoring.copy_reason || ""; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmButton { objectName: "adoptGoogleDraftSource"; Layout.fillWidth: true; text: "Edit Google draft…"; textAlignment: Text.AlignLeft; enabled: backend.draftAuthoring.adopt_allowed || false; onClicked: root.confirmSourceAction("adopt") }
            Text { Layout.fillWidth: true; visible: !backend.draftAuthoring.adopt_allowed && (backend.draftAuthoring.adopt_reason || "").length > 0; text: backend.draftAuthoring.adopt_reason || ""; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            FmButton { objectName: "deleteGoogleDraftSource"; Layout.fillWidth: true; text: "Delete Google draft permanently…"; textAlignment: Text.AlignLeft; foreground: Theme.error; enabled: backend.draftAuthoring.delete_allowed || false; onClicked: root.confirmSourceAction("delete") }
            Text { Layout.fillWidth: true; visible: !backend.draftAuthoring.delete_allowed && (backend.draftAuthoring.delete_reason || "").length > 0; text: backend.draftAuthoring.delete_reason || ""; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            Text { Layout.fillWidth: true; visible: (backend.draftAuthoring.status || "").length > 0; text: backend.draftAuthoring.status || ""; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
        }
    }
    FmConfirmDialog {
        id: sourceActionDialog; objectName: "draftSourceActionDialog"
        titleObjectName: "draftSourceActionTitle"; messageObjectName: "draftSourceActionDisclosure"
        property var capturedContext: ({})
        title: capturedContext.action === "copy" ? "Create a plain-text copy?" : capturedContext.action === "adopt" ? "Edit this Google draft?" : "Delete this draft permanently?"
        message: (capturedContext.subject || "(No subject)") + "\n" + ((capturedContext.primary_from || {}).email || "") + "\n\n" + (capturedContext.action === "copy"
            ? "Keep the original recipients, extracted text and verified files in a separate draft. HTML layout and reply-thread linkage are omitted; no signature is added. Your account's normal draft sync applies."
            : capturedContext.action === "adopt" ? "Check the latest Google version and open it for editing. Opening alone does not upload. Your first edit uses normal draft sync; a detected conflict needs your review."
            : "Permanently delete this exact Google draft after checking its current version. Saved sources and the operation receipt remain available. An uncertain result will need review before any retry.")
        destructive: capturedContext.action === "delete"
        actionText: capturedContext.action === "copy" ? "Create copy" : capturedContext.action === "adopt" ? "Open editor" : "Delete permanently"
        actionObjectName: "confirmDraftSourceAction"; cancelObjectName: "cancelDraftSourceAction"
        onAccepted: backend.confirmDraftSourceAction(capturedContext)
    }
    FmConfirmDialog {
        id: deleteAbandonDialog; objectName: "draftDeletionAbandonDialog"
        property var capturedContext: ({})
        title: "Abandon this deletion?"
        message: "Stop this prepared deletion before it is sent to Google. The Google draft and saved source stay available."
        actionText: "Abandon deletion"; actionObjectName: "confirmAbandonDraftDeletion"
        onAccepted: backend.abandonDraftDeletion(capturedContext)
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
                Text { text: "Saved Google draft sources"; Layout.fillWidth: true; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(15); wrapMode: Text.Wrap }
                FmButton { objectName: "closeDraftSourceHistory"; text: "Close"; onClicked: sourceHistory.close() }
            }
            Text { Layout.fillWidth: true; text: "Export makes a verified local copy. No external content is fetched."; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            ScrollView {
                ScrollBar.vertical: FmScrollBar {}
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
                                Text { Layout.fillWidth: true; text: "Source " + sourceVersion.modelData.source_revision + " · " + sourceVersion.modelData.source_state; wrapMode: Text.Wrap; textFormat: Text.PlainText; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(13) }
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
                                        text: "Copy file " + (index + 1); outlined: true; font.pointSize: Theme.fontPoints(12)
                                        tip: "Export this retained file from source " + sourceVersion.modelData.source_revision
                                        enabled: !backend.draftSources.exporting
                                        onClicked: backend.exportDraftSource(sourceVersion.modelData.source_revision, modelData)
                                    }
                                }
                            }
                        }
                    }
                    Repeater {
                        model: backend.draftSources.delete_operations || []
                        RowLayout {
                            required property var modelData
                            Layout.fillWidth: true; spacing: 8
                            Text { Layout.fillWidth: true; text: "Deletion · source " + modelData.source_revision + " · " + modelData.state; textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
                            FmButton { objectName: "inspectDraftDeletion_" + parent.modelData.request_id; text: "Inspect"; onClicked: root.inspectDeletion(parent.modelData.request_id) }
                        }
                    }
                    FmButton { text: "Latest sources and receipts"; visible: (backend.draftSources.offset || 0) > 0; enabled: !backend.draftSources.loading; onClicked: backend.loadDraftSources() }
                    FmButton {
                        objectName: "moreDraftSources"; text: "Older sources and receipts"
                        visible: (backend.draftSources.next_offset !== undefined && backend.draftSources.next_offset !== null) || (backend.draftSources.delete_next_offset !== undefined && backend.draftSources.delete_next_offset !== null)
                        enabled: !backend.draftSources.loading
                        onClicked: backend.loadDraftSources(true)
                    }
                }
            }
            Text { objectName: "draftSourceExportStatus"; Layout.fillWidth: true; text: backend.draftSources.status || (backend.draftSources.loading ? "Loading saved sources…" : ""); textFormat: Text.PlainText; wrapMode: Text.Wrap; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12) }
            FmButton { objectName: "showDraftSourceFolder"; text: "Show copy in folder"; visible: ((backend.draftSources.export || {}).path || "").length > 0; onClicked: backend.showDraftSourceFolder() }
        }
    }
    Menu {
        id: attachmentMenu
        objectName: "draftAttachmentMenu"
        MenuItem { objectName: "draftAttachLocalFile"; text: "Choose files…"; enabled: !root.readOnly && !backend.attachmentImport.pending; onTriggered: root.chooseLocalAttachment() }
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
        title: "Attach files"
        fileMode: FileDialog.OpenFiles
        onAccepted: backend.importDraftAttachments(selectedFiles, targetAccount, targetDraft)
    }
    FmConfirmDialog {
        id: discardDialog; objectName: "discardDraftDialog"
        property var capturedContext: ({})
        title: "Discard this draft?"
        message: "This draft will be discarded. You can undo for a few seconds afterwards."
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
        id: googleDraftDialog; objectName: "googleDraftReplaceDialog"
        property var capturedContext: ({})
        title: "Replace the Google draft?"
        message: "Upload this saved version to Google. An edit made in another client since the last check may be overwritten. Your retained versions stay available."
        actionText: "Replace"; destructive: true
        actionObjectName: "confirmReplaceGoogleDraft"; cancelObjectName: "cancelReplaceGoogleDraft"
        onAccepted: backend.replaceGoogleDraft(capturedContext)
    }
    FmConfirmDialog {
        id: conflictDialog; objectName: "draftConflictDialog"
        title: "Save your version?"
        message: "Replace the saved draft with your body, formatting, signature and attachments. If it changes again while saving, you can review the new conflict."
        actionText: "Save my version"; actionObjectName: "confirmSaveMyDraft"
        onAccepted: backend.saveMyDraftVersion()
    }
}
