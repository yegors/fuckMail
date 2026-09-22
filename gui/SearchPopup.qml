import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Effects

Popup {
    id: popup
    objectName: "mailSearchPopup"
    signal moreFiltersRequested()
    property var senderOptions: []
    readonly property bool draftScope: backend.folder === "drafts"
    readonly property bool sentScope: backend.folder === "sent"
    readonly property bool inboxScope: !draftScope && !sentScope
    readonly property string currentYear: new Date(backend.clockNow).getUTCFullYear().toString()
    function dateBounds(offset, older) {
        const date = new Date(backend.clockNow)
        const end = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate() - offset + 1))
        const start = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate() - offset))
        return {after: older ? "" : start.toISOString().slice(0,10), before: (older ? start : end).toISOString().slice(0,10)}
    }
    function dateSelected(offset, older) {
        const range = dateBounds(offset, older)
        return (backend.filters.after || "") === range.after && (backend.filters.before || "") === range.before
    }
    function selectDate(offset, older) {
        const filters = Object.assign({}, backend.filters), range = dateBounds(offset, older), selected = dateSelected(offset, older)
        delete filters.after; delete filters.before
        if (!selected) { if (range.after) filters.after = range.after; filters.before = range.before }
        backend.applyFilters(filters)
    }
    width: Math.min(440, parent ? parent.width - 24 : 440)
    height: Math.min(contents.implicitHeight + 32, parent ? parent.height - y - 12 : 560)
    padding: 16
    focus: false
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    function setFilter(key, value) {
        const filters = Object.assign({}, backend.filters)
        if (filters[key] === value) delete filters[key]
        else filters[key] = value
        backend.applyFilters(filters)
    }
    function removeFilter(key) {
        const filters = Object.assign({}, backend.filters)
        delete filters[key]
        backend.applyFilters(filters)
    }
    onOpened: {
        const counts = {}, senders = []
        for (const row of backend.conversations) {
            if (!row.sender_email) continue
            if (!counts[row.sender_email]) { counts[row.sender_email] = 0; senders.push({name: row.sender_name || row.sender_email, email: row.sender_email}) }
            counts[row.sender_email]++
        }
        senderOptions = senders.sort((a,b) => counts[b.email] - counts[a.email] || a.name.localeCompare(b.name)).slice(0,7)
    }
    Connections { target: backend; function onAccountChanged() { popup.close() } }
    component Label: Text { color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(11) } Layout.fillWidth: true }
    component Chip: FmButton {
        id: chip
        implicitHeight: 28; implicitWidth: Math.min(popup.availableWidth, contentItem.implicitWidth + leftPadding + rightPadding)
        font.pixelSize: Theme.fontSize(12)
        leftPadding: 10; rightPadding: 10
        foreground: selected ? Theme.canvas : Theme.inkAlpha(.7)
        font.weight: Font.Normal
        background: Rectangle { radius: 14; color: chip.selected ? Theme.text : chip.hovered ? Theme.hover : "transparent"; border.color: chip.activeFocus ? Theme.focusBorder : chip.selected ? Theme.text : Theme.controlBorder }
    }
    component QuickFilter: Chip {
        implicitHeight: 26; leftPadding: 8; rightPadding: 8
        foreground: selected ? Theme.canvas : Theme.inkAlpha(.75)
        background: Rectangle { radius: 4; color: parent.selected ? Theme.text : parent.hovered ? Theme.hover : Theme.selected; border.width: parent.activeFocus ? 1 : 0; border.color: Theme.focusBorder }
    }
    background: Rectangle {
        color: Theme.surface; border.color: Theme.controlBorder; radius: 8
        layer.enabled: GraphicsInfo.api !== GraphicsInfo.Software
        layer.effect: MultiEffect { shadowEnabled: true; shadowColor: Theme.shadowAlpha(.6); shadowBlur: 1; shadowVerticalOffset: 24 }
    }
    contentItem: ScrollView {
                ScrollBar.vertical: FmScrollBar {}
        clip: true
        contentWidth: availableWidth
        ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
        ColumnLayout {
            id: contents
            width: popup.availableWidth
            spacing: 14
            RowLayout {
                Layout.fillWidth: true
                Text { text: "Search " + (backend.currentAccount.name || "mail") + " — " + (popup.draftScope ? "drafts" : "sender, words, or filters"); color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) } elide: Text.ElideRight; Layout.fillWidth: true }
                FmButton { text: "Esc"; implicitHeight: 21; implicitWidth: 31; leftPadding: 4; rightPadding: 4; font.pixelSize: Theme.fontSize(11); outlined: true; onClicked: popup.close() }
            }
            ColumnLayout {
                Layout.fillWidth: true; spacing: 8
                Label { text: "Looking in" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater {
                        model: [{name:"Inbox",value:"inbox"},{name:"Sent",value:"sent"},{name:"Drafts",value:"drafts"}]
                        Chip { required property var modelData; objectName: "searchScope_" + modelData.value; text: modelData.name; selected: backend.folder === modelData.value; onClicked: backend.folder = modelData.value }
                    }
                    Chip { visible: backend.folder === "all" || backend.folder === "trash"; text: backend.folder === "trash" ? "Trash" : "All mail"; selected: true }
                }
            }
            Flow {
                visible: popup.inboxScope; Layout.fillWidth: true; spacing: 6
                QuickFilter { visible: popup.senderOptions.length > 0; text: "from:" + (popup.senderOptions.length ? popup.senderOptions[0].name : ""); selected: popup.senderOptions.length > 0 && backend.filters.sender === popup.senderOptions[0].email; tip: popup.senderOptions.length ? popup.senderOptions[0].email : ""; onClicked: popup.setFilter("sender",popup.senderOptions[0].email) }
                QuickFilter { visible: backend.categories.length > 0; text: "label:" + (backend.categories.length ? backend.categories[0].name : ""); selected: backend.categories.length > 0 && backend.category === backend.categories[0].id; onClicked: backend.category = selected ? "" : backend.categories[0].id }
                QuickFilter { objectName: "searchQuickAttachment"; text: "has:attachment"; selected: backend.filters.has_attachment === true; onClicked: popup.setFilter("has_attachment",true) }
                QuickFilter { objectName: "searchQuickDelayed"; text: "is:delayed"; selected: backend.filters.status === "held"; onClicked: popup.setFilter("status","held") }
                QuickFilter { objectName: "searchQuickBefore"; text: "before:" + popup.currentYear; selected: backend.filters.before === popup.currentYear + "-01-01"; tip: "Before January 1, " + popup.currentYear + " · UTC"; onClicked: { const filters=Object.assign({},backend.filters); delete filters.after; if (selected) delete filters.before; else filters.before=popup.currentYear+"-01-01"; backend.applyFilters(filters) } }
            }
            ColumnLayout {
                visible: !popup.draftScope && (!!backend.filters.exact_sender || !!backend.filters.kind || !!backend.filters.after || !!backend.filters.before || (popup.sentScope && (!!backend.filters.sender || !!backend.category || !!backend.filters.status || backend.filters.has_attachment !== undefined)))
                Layout.fillWidth: true; spacing: 8
                Label { text: "Active filters" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Chip { objectName: "activeExactSenderFilter"; visible: !!backend.filters.exact_sender; text: "From: " + (backend.filters.exact_sender || ""); iconName: "close"; iconSize: 10; selected: true; tip: "Remove exact sender filter: " + (backend.filters.exact_sender || ""); onClicked: popup.removeFilter("exact_sender") }
                    Chip { objectName: "activeKindFilter"; visible: !!backend.filters.kind; text: ({people:"People",automated:"Automated",unknown:"Unclassified"})[backend.filters.kind] || "Sender kind"; iconName: "close"; iconSize: 10; selected: true; tip: "Remove sender kind filter"; onClicked: popup.removeFilter("kind") }
                    Chip { visible: !!backend.filters.after; text: "After: " + (backend.filters.after || ""); iconName: "close"; iconSize: 10; selected: true; tip: "On or after this date · UTC"; onClicked: popup.removeFilter("after") }
                    Chip { visible: !!backend.filters.before; text: "Before: " + (backend.filters.before || ""); iconName: "close"; iconSize: 10; selected: true; tip: "Before this date · UTC"; onClicked: popup.removeFilter("before") }
                    Chip { visible: popup.sentScope && !!backend.filters.sender; text: "From: " + (backend.filters.sender || ""); iconName: "close"; iconSize: 10; selected: true; onClicked: popup.removeFilter("sender") }
                    Chip { visible: popup.sentScope && !!backend.category; text: "Label: " + ((backend.categories.find(c => c.id === backend.category) || {}).name || backend.category); iconName: "close"; iconSize: 10; selected: true; onClicked: backend.category = "" }
                    Chip { visible: popup.sentScope && !!backend.filters.status; text: ({review:"To review",held:"Delayed",done:"Done"})[backend.filters.status] || "Status"; iconName: "close"; iconSize: 10; selected: true; onClicked: popup.removeFilter("status") }
                    Chip { visible: popup.sentScope && backend.filters.has_attachment !== undefined; text: backend.filters.has_attachment ? "Has attachment" : "No attachments"; iconName: "close"; iconSize: 10; selected: true; onClicked: popup.removeFilter("has_attachment") }
                }
            }
            ColumnLayout {
                visible: popup.inboxScope && popup.senderOptions.length > 0
                Layout.fillWidth: true; spacing: 8
                Label { text: "From" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater { model: popup.senderOptions; Chip { required property var modelData; text: modelData.name; tip: modelData.email; selected: backend.filters.sender === modelData.email; onClicked: popup.setFilter("sender",modelData.email) } }
                }
            }
            ColumnLayout {
                visible: popup.inboxScope && backend.categories.length > 0
                Layout.fillWidth: true; spacing: 8
                Label { text: "Category" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater { model: backend.categories; Chip { required property var modelData; text: modelData.name; selected: backend.category === modelData.id; onClicked: backend.category = selected ? "" : modelData.id } }
                }
            }
            ColumnLayout {
                visible: popup.inboxScope
                Layout.fillWidth: true; spacing: 8
                Label { text: "Status" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Repeater { model: [{name:"To review",value:"review"},{name:"Delayed",value:"held"},{name:"Done",value:"done"}]; Chip { required property var modelData; text: modelData.name; selected: backend.filters.status === modelData.value; onClicked: popup.setFilter("status",modelData.value) } }
                }
            }
            ColumnLayout {
                visible: popup.sentScope; Layout.fillWidth: true; spacing: 8
                Label { text: "Sent" }
                Flow {
                    Layout.fillWidth: true; spacing: 6
                    Chip { objectName: "searchSentToday"; text: "Today"; selected: popup.dateSelected(0,false); tip: "Sent today · UTC"; onClicked: popup.selectDate(0,false) }
                    Chip { objectName: "searchSentYesterday"; text: "Yesterday"; selected: popup.dateSelected(1,false); tip: "Sent yesterday · UTC"; onClicked: popup.selectDate(1,false) }
                    Chip { objectName: "searchSentEarlier"; text: "Earlier"; selected: popup.dateSelected(1,true); tip: "Sent before yesterday · UTC"; onClicked: popup.selectDate(1,true) }
                }
            }
            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.line }
            RowLayout {
                Layout.fillWidth: true
                Text {
                    Layout.fillWidth: true; wrapMode: Text.Wrap
                    text: popup.draftScope ? "Saved on this machine" : "Indexed " + ((backend.currentAccount.coverage || {}).indexed_messages || 0) + " of " + (backend.currentAccount.message_count || 0) + " messages"
                    color: Theme.subtle; font { family: Theme.fontFamily; pixelSize: Theme.fontSize(12) }
                }
                FmButton { objectName: "mailFiltersButton"; visible: !popup.draftScope; iconName: "filter"; iconSize: 13; implicitWidth: 26; implicitHeight: 25; leftPadding: 6; rightPadding: 6; tip: "More filters · sender, attachments and dates"; onClicked: popup.moreFiltersRequested() }
                FmButton { text: "Clear filters"; foreground: Theme.text; font.pixelSize: Theme.fontSize(12); implicitHeight: 25; onClicked: { backend.applyFilters({}); backend.category = ""; backend.query = "" } }
            }
        }
    }
}
