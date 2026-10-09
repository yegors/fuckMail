// A To / Cc / Bcc row in ReplyComposer: recipient chips from backend.draft
// plus a text entry with address suggestions from backend.recipientSuggestions().
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: field
    required property string label
    required property string fieldName
    property bool showLinks: false
    signal ccRequested()
    signal bccRequested()
    readonly property var recipients: backend.draft[fieldName] || []
    property string identity: backend.account + "/" + (backend.activeDraftId || backend.selectedId)
    readonly property bool popupOpen: suggestions.opened
    property var candidates: []
    property string ticket: ""
    property int highlighted: 0
    function placeSuggestions() {
        if (!suggestions.parent) return
        const position = entry.mapToItem(suggestions.parent, 0, entry.height + 6)
        suggestions.x = Math.max(12, Math.min(position.x, suggestions.parent.width - suggestions.width - 12))
        suggestions.y = Math.max(12, Math.min(position.y, suggestions.parent.height - suggestions.implicitHeight - 12))
    }
    onWidthChanged: if (suggestions.opened) placeSuggestions()
    function excluded() {
        let values = []
        for (const key of ["to_recipients", "cc_recipients", "bcc_recipients"])
            for (const recipient of (backend.draft[key] || [])) values.push(recipient.email.toLowerCase())
        return values
    }
    function focusEntry() { entry.forceActiveFocus() }
    function resetSuggestions() { lookup.stop(); ticket = ""; candidates = []; highlighted = 0; suggestions.close() }
    function showCandidates(rows) {
        const used = excluded(), value = entry.text.trim()
        let next = rows.filter(row => used.indexOf(row.email.toLowerCase()) < 0)
        if (/^[^\s<>;,]+@[^\s<>;,]+$/.test(value) && used.indexOf(value.toLowerCase()) < 0 && !next.some(row => row.email.toLowerCase() === value.toLowerCase()))
            next.push({ name: "", email: value, raw: true })
        candidates = next; highlighted = 0
        if (next.length && entry.activeFocus) suggestions.open(); else suggestions.close()
    }
    // While a lookup for the new text is pending, the rows already shown
    // narrow to those that still match, so typing never blanks the list.
    function narrow() {
        const value = entry.text.trim().toLowerCase()
        if (!value.length) { resetSuggestions(); return }
        showCandidates(candidates.filter(row => !row.raw && (String(row.email || "").toLowerCase().indexOf(value) >= 0 || String(row.name || "").toLowerCase().indexOf(value) >= 0)))
    }
    function choose(index) {
        if (index < 0 || index >= candidates.length) return false
        const row = candidates[index]
        if (!backend.addDraftAddress(fieldName, row.email, row.name || "")) return false
        entry.text = ""; resetSuggestions(); entry.forceActiveFocus(); return true
    }
    implicitHeight: Math.max(24, chips.implicitHeight) + 14
    function commit() {
        if (!entry.text.trim().length) return true
        if (!backend.addDraftRecipient(fieldName, entry.text)) return false
        entry.text = ""; resetSuggestions(); return true
    }
    onIdentityChanged: { resetSuggestions(); entry.text = backend.recipientInputs[fieldName] || "" }
    Component.onCompleted: entry.text = backend.recipientInputs[fieldName] || ""
    onVisibleChanged: if (!visible) resetSuggestions()
    Timer {
        id: lookup; interval: 50
        onTriggered: if (entry.activeFocus && entry.text.trim().length) field.ticket = backend.recipientSuggestions(entry.text.trim(), field.excluded())
    }
    Connections {
        target: backend
        function onDraftChanged() {
            const text = backend.recipientInputs[field.fieldName] || ""
            if (entry.text !== text) entry.text = text
        }
        function onRecipientSuggestionsReply(replyTicket, result, ok) {
            if (!field.ticket.length || replyTicket !== field.ticket || result.account !== backend.account || result.query !== entry.text.trim().toLowerCase()) return
            field.ticket = ""
            if (ok && entry.activeFocus) field.showCandidates(result.suggestions || [])
        }
    }
    Rectangle { y: Theme.snapY(parent, parent.height-1); width: parent.width; height: Theme.ruleWidth; color: Theme.line }
    Text { x: 0; y: 7; width: 44; height: 24; verticalAlignment: Text.AlignVCenter; text: field.label; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); color: Theme.inkAlpha(.45) }
    Flow {
        id: chips
        x: 54; y: 7; width: Math.max(0, parent.width-54-(field.showLinks ? links.width+12 : 0)); spacing: 4
        Repeater {
            model: field.recipients
            Rectangle {
                required property var modelData
                required property int index
                height: 24; width: Math.min(recipientName.implicitWidth+31,chips.width); radius: 5; color: "transparent"; border.color: Theme.controlBorder
                Text { id: recipientName; anchors.left: parent.left; anchors.leftMargin: 9; anchors.right: removeRecipient.left; anchors.rightMargin: 3; anchors.verticalCenter: parent.verticalCenter; text: modelData.name || modelData.email; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; textFormat: Text.PlainText }
                FmButton { id: removeRecipient; anchors.right: parent.right; anchors.rightMargin: 3; anchors.verticalCenter: parent.verticalCenter; implicitWidth: 16; implicitHeight: 18; leftPadding: 2; rightPadding: 2; iconName: "close"; iconSize: 9; tip: "Remove "+modelData.email; enabled: !backend.draftLocked; onClicked: backend.removeDraftRecipient(field.fieldName,index) }
            }
        }
        TextField {
            ContextMenu.menu: null
            TextMenuHandler {}
            id: entry
            objectName: "draftRecipient_"+field.fieldName
            property bool blocksWorkspaceShortcuts: true
            width: Math.min(160,chips.width); height: 24; padding: 0; leftPadding: 0; rightPadding: 0; background: Item {}
            color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12.5); placeholderTextColor: Theme.subtle
            placeholderText: field.recipients.length ? "" : "Name or address"
            // Locked, it keeps the keyboard and refuses edits instead of dropping focus.
            readOnly: backend.draftLocked; selectByMouse: true
            onTextEdited: { backend.stageDraftRecipient(field.fieldName,text); field.ticket = ""; field.narrow(); lookup.restart() }
            onAccepted: field.commit()
            onEditingFinished: { field.resetSuggestions(); if (text.trim().length) field.commit() }
            onActiveFocusChanged: if (activeFocus && text.trim().length) lookup.restart()
            Keys.onPressed: event => {
                if (event.modifiers & (Qt.ControlModifier | Qt.MetaModifier | Qt.AltModifier)) return
                const count = field.candidates.length
                if (event.key === Qt.Key_Escape && suggestions.opened) { field.resetSuggestions(); event.accepted = true }
                else if (count && (event.key === Qt.Key_Down || event.key === Qt.Key_Up)) {
                    field.highlighted = (field.highlighted + count + (event.key === Qt.Key_Down ? 1 : -1)) % count; event.accepted = true
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Tab || event.key === Qt.Key_Comma) {
                    if (count && suggestions.opened) { field.choose(field.highlighted); event.accepted = true }
                    else if (text.trim().length) { field.commit(); event.accepted = true }
                } else if (event.key === Qt.Key_Backspace && !text.length && field.recipients.length) {
                    backend.removeDraftRecipient(field.fieldName, field.recipients.length - 1); event.accepted = true
                }
            }
            Accessible.name: field.label+" recipient"
        }
    }
    Popup {
        id: suggestions; objectName: "recipientSuggestions_" + field.fieldName
        parent: Overlay.overlay; padding: 5; focus: false
        width: Math.min(460, parent ? parent.width - 24 : 460)
        implicitHeight: suggestionRows.implicitHeight + topPadding + bottomPadding
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        onAboutToShow: field.placeSuggestions()
        onImplicitHeightChanged: if (opened) field.placeSuggestions()
        Connections {
            target: suggestions.parent
            function onWidthChanged() { if (suggestions.opened) field.placeSuggestions() }
            function onHeightChanged() { if (suggestions.opened) field.placeSuggestions() }
        }
        background: Rectangle { color: Theme.surface; border.color: Theme.controlBorder; radius: 8 }
        contentItem: Column {
            id: suggestionRows; spacing: 2
            Repeater {
                model: field.candidates
                ItemDelegate {
                    id: candidate; required property var modelData; required property int index
                    objectName: "recipientSuggestion_" + field.fieldName + "_" + index
                    width: suggestionRows.width; height: Math.max(50, candidateText.implicitHeight + 12)
                    focusPolicy: Qt.NoFocus; padding: 6
                    onHoveredChanged: if (hovered) field.highlighted = index
                    onClicked: field.choose(index)
                    Accessible.name: (modelData.name || modelData.email) + ", " + modelData.email + (modelData.account !== backend.account && modelData.account_name ? ", " + modelData.account_name : "")
                    background: Rectangle { radius: 5; color: candidate.index === field.highlighted ? Theme.inkAlpha(.06) : "transparent" }
                    contentItem: RowLayout {
                        spacing: 10
                        Rectangle {
                            width: 28; height: 28; radius: 14; color: Theme.inkAlpha(.06)
                            Text { anchors.centerIn: parent; text: candidate.modelData.raw ? "@" : (candidate.modelData.name || candidate.modelData.email).trim().split(/\s+/).slice(0, 2).map(word => word.slice(0, 1)).join("").toUpperCase(); color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5) }
                        }
                        Column {
                            id: candidateText; Layout.fillWidth: true; spacing: 2
                            Text { width: parent.width; text: candidate.modelData.raw ? "Use “" + candidate.modelData.email + "”" : candidate.modelData.name || candidate.modelData.email; color: Theme.text; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(12); elide: Text.ElideRight; textFormat: Text.PlainText }
                            Text { width: parent.width; text: candidate.modelData.raw ? "Enter this address" : candidate.modelData.email; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); elide: Text.ElideRight; textFormat: Text.PlainText }
                        }
                        Text { visible: !candidate.modelData.raw && candidate.modelData.account !== backend.account; Layout.maximumWidth: 95; text: candidate.modelData.account_name || ""; color: Theme.subtle; font.family: Theme.fontFamily; font.pointSize: Theme.fontPoints(10.5); elide: Text.ElideRight; textFormat: Text.PlainText }
                    }
                }
            }
        }
    }
    Row {
        id: links
        visible: field.showLinks; anchors.right: parent.right; y: 7; spacing: 10
        FmButton { objectName: "draftShowCc"; text: "Cc"; implicitHeight: 24; leftPadding: 0; rightPadding: 0; font.pointSize: Theme.fontPoints(11.5); onClicked: field.ccRequested() }
        FmButton { objectName: "draftShowBcc"; text: "Bcc"; implicitHeight: 24; leftPadding: 0; rightPadding: 0; font.pointSize: Theme.fontPoints(11.5); onClicked: field.bccRequested() }
    }
}
